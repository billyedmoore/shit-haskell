#include "HsUnixConfig.h"
#include <stdint.h>
int64_t c_package_include_probe(void) {
#ifdef HAVE_GETPWNAM_R
  return 1;
#else
  return 0;
#endif
}
