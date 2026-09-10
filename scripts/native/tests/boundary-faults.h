#ifndef PTY_RUNTIME_BOUNDARY_FAULTS_H
#define PTY_RUNTIME_BOUNDARY_FAULTS_H
#include "../bridge.h"
/* Only separately compiled bridge objects redirect calls to the shim. Native
 * handles and successful operations always belong to the real pinned engine. */
extern const char *fault;
extern int fault_key;
extern size_t fault_hits, live_allocations;
void inject(const char *, int);
void checked_free(RuntimeTerminal *);
RuntimeTerminal *rt_new(uint16_t, uint16_t, size_t, size_t, size_t);
int rt_feed(RuntimeTerminal *, const uint8_t *, size_t, uint8_t *, size_t, size_t *);
int rt_resize(RuntimeTerminal *, uint16_t, uint16_t);
int rt_compress(RuntimeTerminal *);
int rt_cell(RuntimeTerminal *, int, uint16_t, uint32_t, uint32_t *, size_t, size_t *, RuntimeStyle *);
int rt_rows(RuntimeTerminal *, size_t *, size_t *);
int rt_checkpoint(RuntimeTerminal *, uint8_t *, size_t, size_t *);
RuntimeTerminal *rt_restore(const uint8_t *, size_t, size_t, size_t, size_t, int *);
int rt_history(RuntimeTerminal *, size_t *);
int rt_verify_format(const uint8_t *, size_t, uint8_t *, size_t, size_t *);
#endif
