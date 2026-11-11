/* The RTS functions library C code calls (see include/Rts.h), linked into every program that has C
   sources. They are weak, so a definition elsewhere wins. */
#include "Rts.h"

__attribute__((weak)) void blockUserSignals(void) {}
__attribute__((weak)) void unblockUserSignals(void) {}
__attribute__((weak)) void stopTimer(void) {}
__attribute__((weak)) void startTimer(void) {}
__attribute__((weak)) void hs_init(int *argc, char **argv[]) { (void)argc; (void)argv; }
__attribute__((weak)) void hs_exit(void) {}
__attribute__((weak)) void hs_perform_gc(void) {}
__attribute__((weak)) void hs_free_stable_ptr(HsStablePtr sp) { (void)sp; }
__attribute__((weak)) void hs_free_fun_ptr(HsFunPtr fp) { (void)fp; }

/* The termios and signal helpers of base's HsBase.h (System.Posix.Internals). */
#include <termios.h>
#include <signal.h>

__attribute__((weak)) HsInt __hscore_sizeof_termios(void) { return sizeof(struct termios); }
__attribute__((weak)) HsInt __hscore_sizeof_sigset_t(void) { return sizeof(sigset_t); }
__attribute__((weak)) int __hscore_echo(void) { return ECHO; }
__attribute__((weak)) int __hscore_tcsanow(void) { return TCSANOW; }
__attribute__((weak)) int __hscore_icanon(void) { return ICANON; }
__attribute__((weak)) int __hscore_vmin(void) { return VMIN; }
__attribute__((weak)) int __hscore_vtime(void) { return VTIME; }
__attribute__((weak)) int __hscore_sigttou(void) { return SIGTTOU; }
__attribute__((weak)) int __hscore_sig_block(void) { return SIG_BLOCK; }
__attribute__((weak)) int __hscore_sig_setmask(void) { return SIG_SETMASK; }
__attribute__((weak)) tcflag_t __hscore_lflag(struct termios *ts) { return ts->c_lflag; }
__attribute__((weak)) void __hscore_poke_lflag(struct termios *ts, tcflag_t t) { ts->c_lflag = t; }
__attribute__((weak)) unsigned char *__hscore_ptr_c_cc(struct termios *ts) { return (unsigned char *)&ts->c_cc; }

/* The terminal settings saved for the standard descriptors (restored at exit by GHC's RTS). */
static void *saved_termios[3];
__attribute__((weak)) void *__hscore_get_saved_termios(int fd) { return (fd >= 0 && fd < 3) ? saved_termios[fd] : 0; }
__attribute__((weak)) void __hscore_set_saved_termios(int fd, void *ts) { if (fd >= 0 && fd < 3) saved_termios[fd] = ts; }
