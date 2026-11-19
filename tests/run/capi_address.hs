-- A capi import of a function's address, used as a C finalizer (zlib's
-- `foreign import capi "hs-zlib.h &_hs_zlib_inflateEnd" c_inflateEnd :: FinalizerPtr StreamState`),
-- and finalizing weak pointers and foreign pointers explicitly (zlib's `finalizeForeignPtr`).
{-# LANGUAGE CApiFFI #-}
import Data.Int (Int64)
import Foreign.ForeignPtr (FinalizerPtr, newForeignPtr, finalizeForeignPtr, withForeignPtr)
import Foreign.Marshal.Alloc (mallocBytes)
import Foreign.Ptr (nullFunPtr)
import Foreign.Storable (poke, peek)
import Data.IORef
import System.Mem.Weak (mkWeak, deRefWeak, finalize)

foreign import capi unsafe "capi_address.h &capi_address_finalize" finalizer :: FinalizerPtr Int64
foreign import capi unsafe "capi_address.h capi_address_count" count :: IO Int64

main :: IO ()
main = do
  print (finalizer /= nullFunPtr)
  p <- mallocBytes 8
  fp <- newForeignPtr finalizer p
  withForeignPtr fp $ \q -> poke q 7 >> peek q >>= print
  count >>= print
  finalizeForeignPtr fp
  count >>= print
  finalizeForeignPtr fp
  count >>= print
  ref <- newIORef "alive"
  w <- mkWeak ref "value" (Just (writeIORef ref "finalized"))
  deRefWeak w >>= print
  finalize w
  readIORef ref >>= putStrLn
  deRefWeak w >>= print
  finalize w
  readIORef ref >>= putStrLn
