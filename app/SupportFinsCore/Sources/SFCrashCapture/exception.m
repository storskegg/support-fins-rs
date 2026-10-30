// The uncaught NSException side of native crash capture. The handler runs in ordinary context (the Objective-C
// runtime calls it before it aborts), so it may use Foundation; it writes the exception's name, reason and return
// addresses into the dump, then calls the handler that was installed before it. The abort that follows is a SIGABRT,
// whose section capture.c appends to the same dump.

#import <Foundation/Foundation.h>
#include <stdatomic.h>
#include "internal.h"

static NSUncaughtExceptionHandler *previous_handler = NULL;
static atomic_bool exception_installed = false;

static void on_uncaught(NSException *exception) {
    const char *name = exception.name.UTF8String;
    const char *reason = exception.reason.UTF8String;
    NSArray<NSNumber *> *addresses = exception.callStackReturnAddresses;
    int count = (int)MIN(addresses.count, (NSUInteger)512);
    uintptr_t frames[512];
    for (int i = 0; i < count; i++) frames[i] = (uintptr_t)addresses[(NSUInteger)i].unsignedLongValue;
    sfcc_write_exception(name, name ? strlen(name) : 0, reason, reason ? strlen(reason) : 0, frames, count);
    if (previous_handler) previous_handler(exception);
}

void sfcc_install_exception_handler(void) {
    bool expected = false;
    if (!atomic_compare_exchange_strong(&exception_installed, &expected, true)) return;
    previous_handler = NSGetUncaughtExceptionHandler();
    NSSetUncaughtExceptionHandler(&on_uncaught);
}
