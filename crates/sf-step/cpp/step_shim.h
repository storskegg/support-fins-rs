// The C API over the OpenCascade STEP reader (step_shim.cpp). Node 0 is the root; indices are in the
// order occt-import-js writes its `root` tree and `meshes` array.
#pragma once
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct SfStep SfStep;

// NULL when the kernel could not read the file (occt-import-js `success: false`).
SfStep* sf_step_read(const uint8_t* data, size_t len, double linear_deflection, double angular_deflection);
void sf_step_free(SfStep* r);

size_t sf_step_node_count(const SfStep* r);
const char* sf_step_node_name(const SfStep* r, size_t i);
size_t sf_step_node_mesh_count(const SfStep* r, size_t i);
size_t sf_step_node_mesh(const SfStep* r, size_t i, size_t j);
size_t sf_step_node_child_count(const SfStep* r, size_t i);
size_t sf_step_node_child(const SfStep* r, size_t i, size_t j);

size_t sf_step_mesh_count(const SfStep* r);
const char* sf_step_mesh_name(const SfStep* r, size_t i);
size_t sf_step_mesh_vertex_count(const SfStep* r, size_t i);
const double* sf_step_mesh_positions(const SfStep* r, size_t i);
size_t sf_step_mesh_index_count(const SfStep* r, size_t i);
const uint32_t* sf_step_mesh_indices(const SfStep* r, size_t i);

#ifdef __cplusplus
}
#endif
