/* Stand-in for the RTS's rts/Signals.h, for the Haskell base sources that include it: the
 * signal numbers and the action codes that GHC's RTS defines. Values as in GHC's header (Linux). */
#ifndef HSC_RTS_SIGNALS_H
#define HSC_RTS_SIGNALS_H

#define CONST_SIGINT 2

#define STG_SIG_DFL (-1)
#define STG_SIG_IGN (-2)
#define STG_SIG_ERR (-3)
#define STG_SIG_HAN (-4)
#define STG_SIG_RST (-5)

#endif
