/* Stand-in for the RTS's rts/Signals.h, for the Haskell base sources that include it: the
 * signal numbers and the action codes that GHC's RTS defines. Values as in GHC's header (Linux). */
#ifndef HSC_RTS_SIGNALS_H
#define HSC_RTS_SIGNALS_H

#define CONST_SIGINT 2

#define STG_SIG_DFL 0
#define STG_SIG_IGN 1
#define STG_SIG_HAN 2
#define STG_SIG_RST 3

#endif
