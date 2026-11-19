#include "capi_address.h"
static int64_t finalized = 0;
void capi_address_finalize(void *p) { (void)p; finalized++; }
int64_t capi_address_count(void) { return finalized; }
