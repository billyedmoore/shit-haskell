/* Rts.h: the parts of GHC's runtime interface that library C code uses, as hsc provides them. */
#ifndef RTS_H
#define RTS_H

#include "HsFFI.h"
#include "MachDeps.h"

typedef HsInt StgInt;
typedef HsWord StgWord;
typedef void *StgPtr;
typedef HsWord8 StgWord8;
typedef HsInt32 StgInt32;
typedef HsWord32 StgWord32;
typedef HsInt64 StgInt64;
typedef HsWord64 StgWord64;
typedef double StgDouble;
typedef void *StgStablePtr;

/* Signals and the timer around fork(): hsc's runtime has no timer signal, so these do nothing. */
void blockUserSignals(void);
void unblockUserSignals(void);
void stopTimer(void);
void startTimer(void);

#endif
