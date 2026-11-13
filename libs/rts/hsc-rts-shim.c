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

/* The rest of HsBase.h's helpers (System.Posix.Internals, System.Environment). */
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/stat.h>
#include <unistd.h>

extern char **environ;
__attribute__((weak)) char **__hscore_environ(void) { return environ; }
__attribute__((weak)) int __hscore_get_errno(void) { return errno; }
__attribute__((weak)) void __hscore_set_errno(int e) { errno = e; }
__attribute__((weak)) HsInt __hscore_bufsiz(void) { return BUFSIZ; }
__attribute__((weak)) int __hscore_o_binary(void) { return 0; }
__attribute__((weak)) int __hscore_o_rdonly(void) { return O_RDONLY; }
__attribute__((weak)) int __hscore_o_wronly(void) { return O_WRONLY; }
__attribute__((weak)) int __hscore_o_rdwr(void) { return O_RDWR; }
__attribute__((weak)) int __hscore_o_append(void) { return O_APPEND; }
__attribute__((weak)) int __hscore_o_creat(void) { return O_CREAT; }
__attribute__((weak)) int __hscore_o_excl(void) { return O_EXCL; }
__attribute__((weak)) int __hscore_o_trunc(void) { return O_TRUNC; }
__attribute__((weak)) int __hscore_o_noctty(void) { return O_NOCTTY; }
__attribute__((weak)) int __hscore_o_nonblock(void) { return O_NONBLOCK; }
__attribute__((weak)) int __hscore_f_getfl(void) { return F_GETFL; }
__attribute__((weak)) int __hscore_f_setfl(void) { return F_SETFL; }
__attribute__((weak)) int __hscore_f_setfd(void) { return F_SETFD; }
__attribute__((weak)) long __hscore_fd_cloexec(void) { return FD_CLOEXEC; }
__attribute__((weak)) int __hscore_ftruncate(int fd, off_t where) { return ftruncate(fd, where); }
__attribute__((weak)) int __hscore_setmode(int fd, HsBool toBin) { (void)fd; (void)toBin; return 0; }
__attribute__((weak)) HsInt __hscore_sizeof_stat(void) { return sizeof(struct stat); }
__attribute__((weak)) time_t __hscore_st_mtime(struct stat *st) { return st->st_mtime; }
__attribute__((weak)) off_t __hscore_st_size(struct stat *st) { return st->st_size; }
__attribute__((weak)) mode_t __hscore_st_mode(struct stat *st) { return st->st_mode; }
__attribute__((weak)) dev_t __hscore_st_dev(struct stat *st) { return st->st_dev; }
__attribute__((weak)) ino_t __hscore_st_ino(struct stat *st) { return st->st_ino; }
__attribute__((weak)) int __hscore_stat(char *file, struct stat *buf) { return stat(file, buf); }
__attribute__((weak)) int __hscore_fstat(int fd, struct stat *buf) { return fstat(fd, buf); }
__attribute__((weak)) int __hscore_lstat(const char *fname, struct stat *buf) { return lstat(fname, buf); }
__attribute__((weak)) size_t __hscore_sizeof_siginfo_t(void) { return sizeof(siginfo_t); }
__attribute__((weak)) int __hscore_open(char *file, int how, mode_t mode) { return open(file, how, mode); }
__attribute__((weak)) void *__hscore_from_uintptr(uintptr_t n) { return (void *)n; }
__attribute__((weak)) void *__hscore_from_intptr(intptr_t n) { return (void *)n; }
__attribute__((weak)) uintptr_t __hscore_to_uintptr(void *p) { return (uintptr_t)p; }
__attribute__((weak)) intptr_t __hscore_to_intptr(void *p) { return (intptr_t)p; }
__attribute__((weak)) void errorBelch2(const char *s, char *t) { fprintf(stderr, s, t); fputc('\n', stderr); }
__attribute__((weak)) void debugBelch2(const char *s, char *t) { fprintf(stderr, s, t); }

/* The RTS's capability counts (GHC.Conc's getNumCapabilities reads them): one capability. */
__attribute__((weak)) uint32_t n_capabilities = 1;
__attribute__((weak)) uint32_t enabled_capabilities = 1;

/* The signal the RTS's interval timer uses (System.Posix.Signals blocks it around some calls). */
__attribute__((weak)) int rtsTimerSignal(void) { return SIGVTALRM; }

/* Haskell Program Coverage: programs are not instrumented, so no module is registered. */
__attribute__((weak)) void hs_hpc_module(char *modName, StgWord32 modCount, StgWord32 modHashNo, StgWord64 *tixArr) {
    (void)modName; (void)modCount; (void)modHashNo; (void)tixArr;
}
__attribute__((weak)) HpcModuleInfo *hs_hpc_rootModule(void) { return 0; }
