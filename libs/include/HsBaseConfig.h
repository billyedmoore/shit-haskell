/* Configuration of the base package on x86-64 Linux (no optional C libraries). */
#define HAVE_LIBFFI 0
#define HAVE_WRITEV 1
#define HAVE_EVENTFD 1
#define HAVE_UTIMES 1
#define HAVE_NANOSLEEP 1
#define HAVE_GETTIMEOFDAY 1
#define HAVE_SIGNAL_H 1
/* Sizes and alignments of the C types the Foreign modules name (x86-64 Linux). */
#define SIZEOF_HSFUNPTR 8
#define ALIGNMENT_HSFUNPTR 8
#define SIZEOF_HSSTABLEPTR 8
#define ALIGNMENT_HSSTABLEPTR 8
#define SIZEOF_INT8 1
#define ALIGNMENT_INT8 1
#define SIZEOF_WORD8 1
#define ALIGNMENT_WORD8 1
#define SIZEOF_INT16 2
#define ALIGNMENT_INT16 2
#define SIZEOF_WORD16 2
#define ALIGNMENT_WORD16 2
#define SIZEOF_INT32 4
#define ALIGNMENT_INT32 4
#define SIZEOF_WORD32 4
#define ALIGNMENT_WORD32 4
#define SIZEOF_INT64 8
#define ALIGNMENT_INT64 8
#define SIZEOF_WORD64 8
#define ALIGNMENT_WORD64 8
#define SIZEOF_HSDOUBLE 8
#define ALIGNMENT_HSDOUBLE 8
#define SIZEOF_HSFLOAT 4
#define ALIGNMENT_HSFLOAT 4
/* The C int, as the Haskell type that represents it. */
#define HTYPE_INT Int32
