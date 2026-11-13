{-# LANGUAGE MagicHash, UnboxedTuples, UnliftedFFITypes #-}
-- Unsafe foreign calls given byte arrays directly (as primitive, text and ShellCheck's
-- dependencies call memcpy and memcmp): the C function sees the array's first byte.
module Main where

import Foreign.C.Types
import Foreign.Marshal.Array (withArray)
import Foreign.Ptr
import GHC.Exts
import GHC.IO (IO (..))
import GHC.Word

foreign import ccall unsafe "string.h memcpy"
  c_memcpy :: MutableByteArray# RealWorld -> Ptr Word8 -> CSize -> IO (Ptr ())

foreign import ccall unsafe "string.h memcmp"
  c_memcmp :: ByteArray# -> ByteArray# -> CSize -> IO CInt

foreign import ccall unsafe "string.h memset"
  c_memset :: MutableByteArray# RealWorld -> CInt -> CSize -> IO (Ptr ())

data MBA = MBA (MutableByteArray# RealWorld)
data BA = BA ByteArray#

newBA :: Int -> IO MBA
newBA (I# n) = IO (\s -> case newByteArray# n s of (# s', a #) -> (# s', MBA a #))

freeze :: MBA -> IO BA
freeze (MBA a) = IO (\s -> case unsafeFreezeByteArray# a s of (# s', b #) -> (# s', BA b #))

bytes :: BA -> [Word8]
bytes (BA b) = [W8# (indexWord8Array# b i) | I# i <- [0 .. I# (sizeofByteArray# b) - 1]]

main :: IO ()
main = do
  m1@(MBA a1) <- newBA 6
  _ <- withArray [104, 101, 108, 108, 111, 33 :: Word8] (\p -> c_memcpy a1 p 6)
  b1 <- freeze m1
  print (bytes b1)
  m2@(MBA a2) <- newBA 6
  _ <- c_memset a2 104 6
  b2@(BA r2) <- freeze m2
  let BA r1 = b1
  r <- c_memcmp r1 r2 1
  r' <- c_memcmp r1 r2 6
  print (bytes b2, r, signum r')
