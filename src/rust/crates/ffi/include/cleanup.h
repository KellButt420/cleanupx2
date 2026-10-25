#ifndef CLEANUP_ABI_H
#define CLEANUP_ABI_H
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ABI 3. Length-delimited UTF-8 JSON. Returned buffers are Rust-owned;
   release each exactly once with cln_buffer_free. Inputs are borrowed.
   Handles must stay live throughout polling/cancellation; do not free
   concurrently. See docs/ffi-contract.md for message and error semantics. */
typedef struct ClnOperation ClnOperation;
typedef struct ClnBuffer { uint8_t *ptr; size_t len; } ClnBuffer;
enum {
    CLN_OK = 0, CLN_MESSAGE = 1, CLN_TIMEOUT = 2, CLN_COMPLETE = 3,
    CLN_E_INVALID_ARGUMENT = -1, CLN_E_FAILED = -2, CLN_E_PANIC = -3
};

uint32_t cln_abi_version(void);
void cln_buffer_free(ClnBuffer buffer);
int32_t cln_environment_probe(ClnBuffer *output);
int32_t cln_probe_persistence(const uint8_t *request, size_t length, ClnBuffer *output);
int32_t cln_probe_file_moves(const uint8_t *request, size_t length, ClnBuffer *output);
int32_t cln_inventory_scan_start(const uint8_t *request, size_t length,
                               ClnOperation **operation, ClnBuffer *error);
int32_t cln_uninstall_start(const uint8_t *request, size_t length,
                          ClnOperation **operation, ClnBuffer *error);
int32_t cln_organize_plan_start(const uint8_t *request, size_t length,
                              ClnOperation **operation, ClnBuffer *error);
int32_t cln_organize_apply_start(const uint8_t *request, size_t length,
                               ClnOperation **operation, ClnBuffer *error);
int32_t cln_organize_locations(const uint8_t *request, size_t length, ClnBuffer *output);
int32_t cln_organize_plan(const uint8_t *request, size_t length, ClnBuffer *output);
int32_t cln_organize_apply(const uint8_t *request, size_t length, ClnBuffer *output);
int32_t cln_organize_undo(const uint8_t *request, size_t length, ClnBuffer *output);
int32_t cln_leftovers_scan(const uint8_t *request, size_t length, ClnBuffer *output);
int32_t cln_leftovers_remove(const uint8_t *request, size_t length, ClnBuffer *output);
int32_t cln_history_list(const uint8_t *request, size_t length, ClnBuffer *output);
int32_t cln_history_record_uninstall(const uint8_t *request, size_t length, ClnBuffer *output);
int32_t cln_operation_next(ClnOperation *operation, uint32_t timeout_ms, ClnBuffer *output);
int32_t cln_operation_cancel(ClnOperation *operation);
void cln_operation_free(ClnOperation *operation);

#ifdef __cplusplus
}
#endif
#endif
