// A C face for the parts of Manifold's C++ API that web/cut.js uses, called exactly as the
// manifold-3d 3.5.4 JS bindings (bindings/wasm/helpers.cpp) call them: a MeshGL built from the
// arrays, `Merge()`, `Manifold(MeshGL)`, `BatchBoolean(.., Add)`, `a - b`, `GetMeshGL(-1)`.
#include <cstdint>
#include <cstring>
#include <vector>

#include "manifold/manifold.h"

using manifold::Manifold;
using manifold::MeshGL;
using manifold::OpType;

extern "C" {

struct SfMesh {
  MeshGL gl;
};

uint32_t sfc_reserve_ids(uint32_t n) { return Manifold::ReserveIDs(n); }

// `new Mesh({ numProp: 3, vertProperties, triVerts, faceID?, runIndex?, runOriginalID? })`,
// `mesh.merge()`, `new Manifold(mesh)`. Returns null if Manifold throws. `*status` is
// `Manifold::Error` as an int (0 = NoError), `*empty` is `isEmpty()`.
Manifold* sfc_from_mesh(const float* verts, size_t n_verts, const uint32_t* tris, size_t n_tris,
                        const uint32_t* face_id, const uint32_t* original_id, int* status,
                        int* empty) {
  try {
    MeshGL gl;
    gl.numProp = 3;
    gl.vertProperties.assign(verts, verts + n_verts * 3);
    gl.triVerts.assign(tris, tris + n_tris * 3);
    if (face_id) gl.faceID.assign(face_id, face_id + n_tris);
    if (original_id) {
      gl.runIndex = {0, static_cast<uint32_t>(n_tris * 3)};
      gl.runOriginalID = {*original_id};
    }
    gl.Merge();
    Manifold* m = new Manifold(gl);
    *status = static_cast<int>(m->Status());
    *empty = m->IsEmpty() ? 1 : 0;
    return m;
  } catch (...) {
    return nullptr;
  }
}

void sfc_delete(Manifold* m) { delete m; }

Manifold* sfc_union(Manifold* const* ms, size_t n) {
  try {
    std::vector<Manifold> v;
    for (size_t i = 0; i < n; i++) v.push_back(*ms[i]);
    return new Manifold(Manifold::BatchBoolean(v, OpType::Add));
  } catch (...) {
    return nullptr;
  }
}

Manifold* sfc_subtract(const Manifold* a, const Manifold* b) {
  try {
    return new Manifold(*a - *b);
  } catch (...) {
    return nullptr;
  }
}

SfMesh* sfc_get_mesh(const Manifold* m) {
  try {
    SfMesh* out = new SfMesh;
    out->gl = m->GetMeshGL(-1);
    return out;
  } catch (...) {
    return nullptr;
  }
}

size_t sfc_mesh_num_prop(const SfMesh* m) { return m->gl.numProp; }
size_t sfc_mesh_len_verts(const SfMesh* m) { return m->gl.vertProperties.size(); }
size_t sfc_mesh_len_tris(const SfMesh* m) { return m->gl.triVerts.size(); }
size_t sfc_mesh_len_face_id(const SfMesh* m) { return m->gl.faceID.size(); }
size_t sfc_mesh_len_runs(const SfMesh* m) { return m->gl.runOriginalID.size(); }
size_t sfc_mesh_len_run_index(const SfMesh* m) { return m->gl.runIndex.size(); }
const float* sfc_mesh_verts(const SfMesh* m) { return m->gl.vertProperties.data(); }
const uint32_t* sfc_mesh_tris(const SfMesh* m) { return m->gl.triVerts.data(); }
const uint32_t* sfc_mesh_face_id(const SfMesh* m) { return m->gl.faceID.data(); }
const uint32_t* sfc_mesh_run_index(const SfMesh* m) { return m->gl.runIndex.data(); }
const uint32_t* sfc_mesh_run_original_id(const SfMesh* m) { return m->gl.runOriginalID.data(); }
void sfc_mesh_delete(SfMesh* m) { delete m; }

}  // extern "C"
