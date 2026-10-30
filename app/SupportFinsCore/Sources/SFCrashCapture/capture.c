// Native crash capture: the fatal-signal handler and the raw dump (see include/SFCrashCapture.h).
//
// The dump is text, one fact per line, with length-prefixed blobs for anything that may hold a newline:
//
//   SFNATIVE 1
//   pid 1234
//   time 1793318400                    unix seconds at the crash
//   app <version>
//   section signal                     or `section exception` (exception.m), one or more sections
//   signal 11
//   code 2
//   addr 0x10
//   main-thread 1
//   reg pc 0x... / reg lr / reg fp / reg sp
//   frame 0x...                        the first frame of a signal section is the pc, the rest return addresses
//   frame-lr 0x...                     the second, when lr is not the return address the frame records
//   name <n>\n<n bytes>\n              (exception sections: the exception's name and reason)
//   image 0x<load> 0x<__TEXT size> <UUID> <path>
//   context <n>\n<n bytes>\n           the breadcrumbs and settings JSON
//   end
//
// Everything the handler calls is async-signal-safe (POSIX's list: write, open, close, getpid, time, sigaction, raise,
// sleep) or a Mach call that takes no lock and allocates nothing (vm_read_overwrite, mach_task_self). pthread_self is
// a thread-register read. Nothing else: no malloc, no stdio, no locks, no Swift or Objective-C.

#include "include/SFCrashCapture.h"
#include "internal.h"

#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <mach-o/dyld.h>
#include <mach-o/dyld_images.h>
#include <mach-o/loader.h>
#include <mach/mach.h>
#include <pthread.h>
#include <signal.h>
#include <stdatomic.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>

#define FATAL_COUNT 6
static const int fatal_signals[FATAL_COUNT] = {SIGSEGV, SIGBUS, SIGILL, SIGFPE, SIGABRT, SIGTRAP};
static struct sigaction previous_actions[FATAL_COUNT];

// ---- prepared at install ----

#define PATH_CAP 1024
static char dump_paths[2][PATH_CAP];
static _Atomic int dump_path_index = 0;
static char app_version_text[256];
static int reserve_fd = -1;           // closed to free a descriptor when open() fails with EMFILE
static pthread_t main_thread;
static atomic_bool installed = false;
static pthread_mutex_t install_lock = PTHREAD_MUTEX_INITIALIZER;

// ---- the image table, filled by dyld's add-image callback ----

#define MAX_IMAGES 4096
typedef struct {
    _Atomic uintptr_t header;         // 0: free, or unloaded
    uint64_t text_size;
    uint8_t uuid[16];
    const char *path;                 // dyld's string, alive while the image is loaded
    _Atomic int ready;
} image_entry;
static image_entry images[MAX_IMAGES];
static _Atomic int image_slots = 0;

// ---- the context (breadcrumbs and settings), double-buffered ----

static char context_buffers[2][SFCC_CONTEXT_CAPACITY];
static size_t context_lengths[2];
static _Atomic int context_index = 0;

// ---- the dump being written ----

static _Atomic int handling = 0;      // 0: no crash yet; 1: a thread is writing; 2: written
static int dump_fd = -1;
static int header_written = 0;
static int trailer_written = 0;
static char out_buffer[4096];
static size_t out_length = 0;

// MARK: - writing (async-signal-safe)

static size_t s_len(const char *s) {
    size_t n = 0;
    if (s) while (s[n]) n++;
    return n;
}

static void raw_write(int fd, const char *p, size_t n) {
    while (n > 0) {
        ssize_t w = write(fd, p, n);
        if (w < 0) {
            if (errno == EINTR) continue;
            return;
        }
        p += w;
        n -= (size_t)w;
    }
}

static void out_flush(void) {
    if (out_length && dump_fd >= 0) raw_write(dump_fd, out_buffer, out_length);
    out_length = 0;
}

static void out_bytes(const char *p, size_t n) {
    if (n > sizeof out_buffer) {
        out_flush();
        if (dump_fd >= 0) raw_write(dump_fd, p, n);
        return;
    }
    if (out_length + n > sizeof out_buffer) out_flush();
    memcpy(out_buffer + out_length, p, n);   // memcpy is a plain copy here: no allocation, no lock
    out_length += n;
}

static void out_str(const char *s) { out_bytes(s, s_len(s)); }

static void out_hex(uint64_t v) {
    char b[18];
    int i = 18;
    do {
        b[--i] = "0123456789abcdef"[v & 15];
        v >>= 4;
    } while (v && i > 2);
    b[--i] = 'x';
    b[--i] = '0';
    out_bytes(b + i, (size_t)(18 - i));
}

static void out_dec(int64_t v) {
    char b[24];
    int i = 24;
    uint64_t u = v < 0 ? (uint64_t)(-(v + 1)) + 1 : (uint64_t)v;
    do {
        b[--i] = (char)('0' + u % 10);
        u /= 10;
    } while (u);
    if (v < 0) b[--i] = '-';
    out_bytes(b + i, (size_t)(24 - i));
}

static void out_blob(const char *key, const char *p, size_t n) {
    out_str(key);
    out_str(" ");
    out_dec((int64_t)n);
    out_str("\n");
    out_bytes(p, n);
    out_str("\n");
}

static void out_uuid(const uint8_t *u) {
    char b[36];
    int j = 0;
    for (int i = 0; i < 16; i++) {
        if (i == 4 || i == 6 || i == 8 || i == 10) b[j++] = '-';
        b[j++] = "0123456789ABCDEF"[u[i] >> 4];
        b[j++] = "0123456789ABCDEF"[u[i] & 15];
    }
    out_bytes(b, (size_t)j);
}

/// Open the dump (once) and write its header.
static void begin_dump(void) {
    if (dump_fd < 0) {
        const char *path = dump_paths[atomic_load(&dump_path_index)];
        if (!path[0]) return;
        dump_fd = open(path, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0600);
        if (dump_fd < 0 && reserve_fd >= 0) {     // out of descriptors: give back the reserve one
            close(reserve_fd);
            reserve_fd = -1;
            dump_fd = open(path, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0600);
        }
        if (dump_fd < 0) return;
    }
    if (header_written) return;
    header_written = 1;
    out_str("SFNATIVE ");
    out_dec(SFCC_FORMAT_VERSION);
    out_str("\npid ");
    out_dec(getpid());
    out_str("\ntime ");
    out_dec((int64_t)time(NULL));
    out_str("\napp ");
    out_str(app_version_text);
    out_str("\n");
}

/// The images and the context, once per dump, then `end`.
static void write_trailer(void) {
    if (trailer_written) return;
    trailer_written = 1;
    int n = atomic_load(&image_slots);
    if (n > MAX_IMAGES) n = MAX_IMAGES;
    for (int i = 0; i < n; i++) {
        image_entry *e = &images[i];
        if (!atomic_load(&e->ready)) continue;
        uintptr_t h = atomic_load(&e->header);
        if (!h) continue;
        out_str("image ");
        out_hex(h);
        out_str(" ");
        out_hex(e->text_size);
        out_str(" ");
        out_uuid(e->uuid);
        out_str(" ");
        out_str(e->path ? e->path : "?");
        out_str("\n");
    }
    int c = atomic_load(&context_index);
    size_t len = context_lengths[c];
    if (len > SFCC_CONTEXT_CAPACITY) len = SFCC_CONTEXT_CAPACITY;
    out_blob("context", context_buffers[c], len);
    out_str("end\n");
    out_flush();
}

// MARK: - the stack

/// A pointer without its pointer-authentication bits (the system's arm64e frames sign return addresses).
static uint64_t strip(uint64_t p) { return p & 0x00007FFFFFFFFFFFULL; }

/// Read `n` bytes at `addr` without faulting: false when the memory is not readable.
static int safe_read(uint64_t addr, void *out, size_t n) {
    vm_size_t got = 0;
    kern_return_t kr = vm_read_overwrite(mach_task_self(), (vm_address_t)addr, (vm_size_t)n, (vm_address_t)out, &got);
    return kr == KERN_SUCCESS && got == n;
}

#define MAX_FRAMES 256

static void write_stack(uint64_t pc, uint64_t lr, uint64_t fp) {
    out_str("frame ");
    out_hex(strip(pc));
    out_str("\n");
    uint64_t frame[2];
    int have_first = fp && (fp & 7) == 0 && safe_read(fp, frame, sizeof frame);
    // a leaf function has not pushed its frame: its caller is only in lr
    if (lr && (!have_first || strip(frame[1]) != strip(lr))) {
        // `frame-lr`: lr may also be stale (a function with its frame pushed keeps the return of its last call
        // there); the reader drops it when it names the same function as the pc
        out_str("frame-lr ");
        out_hex(strip(lr));
        out_str("\n");
    }
    for (int i = 0; i < MAX_FRAMES && fp && (fp & 7) == 0; i++) {
        if (!safe_read(fp, frame, sizeof frame)) break;
        uint64_t ret = strip(frame[1]);
        if (!ret) break;
        out_str("frame ");
        out_hex(ret);
        out_str("\n");
        uint64_t next = strip(frame[0]);
        if (next <= fp) break;   // stacks grow down: a caller's frame is above; anything else is corrupt
        fp = next;
    }
}

// MARK: - the handler

static int signal_slot(int sig) {
    for (int i = 0; i < FATAL_COUNT; i++)
        if (fatal_signals[i] == sig) return i;
    return -1;
}

/// A fault the hardware raised (the instruction runs again on return and faults with the default action), as
/// opposed to one sent with kill/raise/abort (raised again here).
static int is_hardware_fault(int sig, const siginfo_t *info) {
    if (sig == SIGABRT || !info) return 0;
    return info->si_code > 0 && info->si_code < SI_USER;
}

static void die(int sig, siginfo_t *info) {
    struct sigaction dfl;
    memset(&dfl, 0, sizeof dfl);
    dfl.sa_handler = SIG_DFL;
    sigemptyset(&dfl.sa_mask);
    sigaction(sig, &dfl, NULL);
    // returning re-runs a faulting instruction, which now takes the default action with the original fault state;
    // a sent signal is sent again (blocked in the handler, it is delivered as the handler returns)
    if (!is_hardware_fault(sig, info)) raise(sig);
}

static void handler(int sig, siginfo_t *info, void *context) {
    int expected = 0;
    if (!atomic_compare_exchange_strong(&handling, &expected, 1)) {
        // another thread crashed first and is writing; give it a moment, then die
        for (int i = 0; i < 3 && atomic_load(&handling) == 1; i++) sleep(1);
        die(sig, info);
        return;
    }
    begin_dump();
    out_str("section signal\nsignal ");
    out_dec(sig);
    out_str("\ncode ");
    out_dec(info ? info->si_code : 0);
    out_str("\naddr ");
    out_hex(info ? (uint64_t)(uintptr_t)info->si_addr : 0);
    out_str("\nmain-thread ");
    out_dec(pthread_equal(pthread_self(), main_thread) ? 1 : 0);
    out_str("\n");
    ucontext_t *uc = (ucontext_t *)context;
    if (uc && uc->uc_mcontext) {
#if defined(__arm64__)
        const struct __darwin_arm_thread_state64 *ss = &uc->uc_mcontext->__ss;
        uint64_t pc = (uint64_t)ss->__pc, lr = (uint64_t)ss->__lr, fp = (uint64_t)ss->__fp, sp = (uint64_t)ss->__sp;
#elif defined(__x86_64__)
        const struct __darwin_x86_thread_state64 *ss = &uc->uc_mcontext->__ss;
        uint64_t pc = ss->__rip, lr = 0, fp = ss->__rbp, sp = ss->__rsp;
#endif
        out_str("reg pc ");
        out_hex(pc);
        out_str("\nreg lr ");
        out_hex(lr);
        out_str("\nreg fp ");
        out_hex(fp);
        out_str("\nreg sp ");
        out_hex(sp);
        out_str("\n");
        write_stack(pc, lr, fp);
    }
    write_trailer();
    out_flush();   // the trailer flushes too, but after an exception section it was already written
    if (dump_fd >= 0) {
        close(dump_fd);
        dump_fd = -1;
    }
    atomic_store(&handling, 2);

    // the handler that was there before ours
    int slot = signal_slot(sig);
    if (slot >= 0) {
        struct sigaction *prev = &previous_actions[slot];
        if (prev->sa_flags & SA_SIGINFO) {
            if (prev->sa_sigaction) {
                sigaction(sig, prev, NULL);
                prev->sa_sigaction(sig, info, context);
            }
        } else if (prev->sa_handler != SIG_DFL && prev->sa_handler != SIG_IGN && prev->sa_handler) {
            sigaction(sig, prev, NULL);
            prev->sa_handler(sig);
        }
    }
    die(sig, info);
}

// MARK: - the uncaught exception (ordinary context: called from exception.m)

void sfcc_write_exception(const char *name, size_t name_len, const char *reason, size_t reason_len,
                          const uintptr_t *frames, int count) {
    int expected = 0;
    if (!atomic_compare_exchange_strong(&handling, &expected, 1)) return;
    begin_dump();
    out_str("section exception\n");
    out_blob("name", name ? name : "", name ? name_len : 0);
    out_blob("reason", reason ? reason : "", reason ? reason_len : 0);
    out_str("main-thread ");
    out_dec(pthread_equal(pthread_self(), main_thread) ? 1 : 0);
    out_str("\n");
    for (int i = 0; i < count; i++) {
        out_str("frame ");
        out_hex(strip(frames[i]));
        out_str("\n");
    }
    write_trailer();
    out_flush();
    // the dump stays open: the abort that follows appends its signal section
    atomic_store(&handling, 0);
}

// MARK: - images

static void read_image(const struct mach_header *mh, image_entry *e) {
    memset(e->uuid, 0, sizeof e->uuid);
    e->text_size = 0;
    if (mh->magic != MH_MAGIC_64) return;
    const uint8_t *p = (const uint8_t *)mh + sizeof(struct mach_header_64);
    for (uint32_t i = 0; i < mh->ncmds; i++) {
        const struct load_command *lc = (const struct load_command *)p;
        if (lc->cmd == LC_UUID) {
            memcpy(e->uuid, ((const struct uuid_command *)lc)->uuid, 16);
        } else if (lc->cmd == LC_SEGMENT_64) {
            const struct segment_command_64 *seg = (const struct segment_command_64 *)lc;
            if (strcmp(seg->segname, "__TEXT") == 0) e->text_size = seg->vmsize;
        }
        p += lc->cmdsize;
    }
}

static void image_added(const struct mach_header *mh, intptr_t slide) {
    (void)slide;
    int i = atomic_fetch_add(&image_slots, 1);
    if (i >= MAX_IMAGES) return;
    image_entry *e = &images[i];
    read_image(mh, e);
    Dl_info info;
    e->path = dladdr(mh, &info) ? info.dli_fname : NULL;
    atomic_store(&e->header, (uintptr_t)mh);
    atomic_store(&e->ready, 1);
}

static void image_removed(const struct mach_header *mh, intptr_t slide) {
    (void)slide;
    int n = atomic_load(&image_slots);
    if (n > MAX_IMAGES) n = MAX_IMAGES;
    for (int i = 0; i < n; i++) {
        uintptr_t h = (uintptr_t)mh;
        if (atomic_compare_exchange_strong(&images[i].header, &h, 0)) return;
    }
}

int sfcc_handlers_in_place(void) {
    int n = 0;
    for (int i = 0; i < FATAL_COUNT; i++) {
        struct sigaction now;
        if (sigaction(fatal_signals[i], NULL, &now) == 0 && (now.sa_flags & SA_SIGINFO) && now.sa_sigaction == handler) n++;
    }
    return n;
}

int sfcc_image_count(void) {
    int n = atomic_load(&image_slots);
    return n > MAX_IMAGES ? MAX_IMAGES : n;
}

// MARK: - install

const char *sfcc_dump_path(void) { return dump_paths[atomic_load(&dump_path_index)]; }

void sfcc_set_context(const char *bytes, size_t length) {
    if (length > SFCC_CONTEXT_CAPACITY) length = SFCC_CONTEXT_CAPACITY;
    int spare = 1 - atomic_load(&context_index);
    if (length && bytes) memcpy(context_buffers[spare], bytes, length);
    context_lengths[spare] = length;
    atomic_store(&context_index, spare);
}

int sfcc_install(const char *dir, const char *app_version) {
    pthread_mutex_lock(&install_lock);
    int next = 1 - atomic_load(&dump_path_index);
    int n = snprintf(dump_paths[next], PATH_CAP, "%s/native-%lld-%d.sfdump", dir, (long long)time(NULL), (int)getpid());
    if (n < 0 || n >= PATH_CAP) {
        dump_paths[next][0] = 0;
        pthread_mutex_unlock(&install_lock);
        return -1;
    }
    atomic_store(&dump_path_index, next);
    snprintf(app_version_text, sizeof app_version_text, "%s", app_version ? app_version : "?");
    for (char *c = app_version_text; *c; c++)
        if (*c == '\n') *c = ' ';
    if (atomic_load(&installed)) {
        pthread_mutex_unlock(&install_lock);
        return 0;
    }
    main_thread = pthread_self();
    reserve_fd = open("/dev/null", O_RDONLY | O_CLOEXEC);
    // dyld itself is not in the list the callback sees (the outermost frame, `start`, is in it)
    struct task_dyld_info dyld_info;
    mach_msg_type_number_t dyld_count = TASK_DYLD_INFO_COUNT;
    if (task_info(mach_task_self(), TASK_DYLD_INFO, (task_info_t)&dyld_info, &dyld_count) == KERN_SUCCESS) {
        const struct dyld_all_image_infos *all = (const struct dyld_all_image_infos *)(uintptr_t)dyld_info.all_image_info_addr;
        if (all && all->dyldImageLoadAddress) {
            int i = atomic_fetch_add(&image_slots, 1);
            if (i < MAX_IMAGES) {
                read_image(all->dyldImageLoadAddress, &images[i]);
                images[i].path = "/usr/lib/dyld";
                atomic_store(&images[i].header, (uintptr_t)all->dyldImageLoadAddress);
                atomic_store(&images[i].ready, 1);
            }
        }
    }
    _dyld_register_func_for_add_image(image_added);
    _dyld_register_func_for_remove_image(image_removed);

    // a stack of the handler's own, so a stack overflow (on this thread) can still be written up
    size_t page = (size_t)getpagesize(), size = 256 * 1024;
    void *mem = mmap(NULL, size + page, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANON, -1, 0);
    if (mem != MAP_FAILED) {
        mprotect(mem, page, PROT_NONE);   // a guard page below it
        stack_t ss = {.ss_sp = (char *)mem + page, .ss_size = size, .ss_flags = 0};
        sigaltstack(&ss, NULL);
    }

    struct sigaction sa;
    memset(&sa, 0, sizeof sa);
    sa.sa_sigaction = handler;
    sa.sa_flags = SA_SIGINFO | SA_ONSTACK;
    sigemptyset(&sa.sa_mask);
    for (int i = 0; i < FATAL_COUNT; i++) sigaddset(&sa.sa_mask, fatal_signals[i]);
    for (int i = 0; i < FATAL_COUNT; i++) sigaction(fatal_signals[i], &sa, &previous_actions[i]);
    sfcc_install_exception_handler();
    atomic_store(&installed, true);
    pthread_mutex_unlock(&install_lock);
    return 0;
}
