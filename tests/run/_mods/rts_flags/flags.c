/* What ghc-lib-parser's C helpers do with GHC's RTS settings and the CAF flag. */
#include <Rts.h>
#include <HsFFI.h>

void rts_flags_touch(HsInt size)
{
    RtsFlags.GcFlags.heapSizeSuggestion = size / BLOCK_SIZE;
}

HsInt rts_flags_keep(void)
{
    setKeepCAFs();
    return keepCAFs ? 1 : 0;
}
