/* HsFFI.h: the Haskell 2010 FFI types and functions, as hsc provides them (x86-64). */
#ifndef HSFFI_H
#define HSFFI_H

#include <stdint.h>
#include <stddef.h>
#include "ghcautoconf.h"

typedef int64_t   HsInt;
typedef int8_t    HsInt8;
typedef int16_t   HsInt16;
typedef int32_t   HsInt32;
typedef int64_t   HsInt64;
typedef uint64_t  HsWord;
typedef uint8_t   HsWord8;
typedef uint16_t  HsWord16;
typedef uint32_t  HsWord32;
typedef uint64_t  HsWord64;
typedef uint32_t  HsChar;
typedef int64_t   HsBool;
typedef double    HsDouble;
typedef float     HsFloat;
typedef void     *HsPtr;
typedef void    (*HsFunPtr)(void);
typedef void     *HsStablePtr;

#define HS_BOOL_FALSE 0
#define HS_BOOL_TRUE 1
#define HS_INT_MIN INT64_MIN
#define HS_INT_MAX INT64_MAX
#define HS_WORD_MAX UINT64_MAX
#define HS_CHAR_MIN 0
#define HS_CHAR_MAX 0x10FFFF

#ifdef __cplusplus
extern "C" {
#endif
void hs_init(int *argc, char **argv[]);
void hs_exit(void);
void hs_perform_gc(void);
void hs_free_stable_ptr(HsStablePtr sp);
void hs_free_fun_ptr(HsFunPtr fp);
#ifdef __cplusplus
}
#endif

#endif
