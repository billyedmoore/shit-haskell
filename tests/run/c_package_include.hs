-- A C file of the program includes a header of a package in scope (unix-compat's cbits include
-- unix's HsUnixConfig.h): the package's include directories are passed to the C compiler.
import Data.Int (Int64)

foreign import ccall unsafe "c_package_include_probe" probe :: IO Int64

main :: IO ()
main = probe >>= \n -> print (n >= 0)
