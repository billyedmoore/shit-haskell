{-# LANGUAGE MagicHash, UnboxedTuples #-}
-- Atomic primitives (primitive, unordered-containers and hlint's dependencies use them): fetch
-- and op on byte arrays, compare and swap of bytes and of array elements, atomic reads and
-- writes, and atomicSwapIORef, with laziness kept.
import GHC.Exts
import GHC.IO (IO (..))
import Data.IORef
import GHC.IORef (atomicSwapIORef)

data MBA = MBA (MutableByteArray# RealWorld)

newMBA :: Int -> IO MBA
newMBA (I# n) = IO $ \s -> case newByteArray# n s of (# s', a #) -> (# s', MBA a #)

fetchOp :: (MutableByteArray# RealWorld -> Int# -> Int# -> State# RealWorld -> (# State# RealWorld, Int# #))
        -> MBA -> Int -> Int -> IO Int
fetchOp f (MBA a) (I# i) (I# v) = IO $ \s -> case f a i v s of (# s', r #) -> (# s', I# r #)

readA :: MBA -> Int -> IO Int
readA (MBA a) (I# i) = IO $ \s -> case atomicReadIntArray# a i s of (# s', r #) -> (# s', I# r #)

writeA :: MBA -> Int -> Int -> IO ()
writeA (MBA a) (I# i) (I# v) = IO $ \s -> (# atomicWriteIntArray# a i v s, () #)

casA :: MBA -> Int -> Int -> Int -> IO Int
casA (MBA a) (I# i) (I# o) (I# n) = IO $ \s -> case casIntArray# a i o n s of (# s', r #) -> (# s', I# r #)

cas8 :: MBA -> Int -> Int -> Int -> IO Int
cas8 (MBA a) (I# i) (I# o) (I# n) = IO $ \s -> case casInt8Array# a i (intToInt8# o) (intToInt8# n) s of
  (# s', r #) -> (# s', I# (int8ToInt# r) #)

data MA a = MA (MutableArray# RealWorld a)

main :: IO ()
main = do
  b <- newMBA 32
  mapM_ (\i -> writeA b i 0) [0 .. 3]
  rs <- mapM (\(f, v) -> fetchOp f b 1 v)
    [(fetchAddIntArray#, 10), (fetchSubIntArray#, 3), (fetchOrIntArray#, 16), (fetchAndIntArray#, 22), (fetchXorIntArray#, 5), (fetchNandIntArray#, 1)]
  print rs
  readA b 1 >>= print
  writeA b 2 42
  c1 <- casA b 2 41 7
  c2 <- casA b 2 42 7
  readA b 2 >>= \v -> print (c1, c2, v)
  writeA b 3 0
  d1 <- cas8 b 24 0 (-3)
  d2 <- cas8 b 24 (-3) 9
  print (d1, d2)
  -- casArray#: success answers the new element, failure the current one.
  MA arr <- IO $ \s -> case newArray# 2# "a" s of (# s', a #) -> (# s', MA a #)
  cur <- IO $ \s -> case readArray# arr 0# s of (# s', x #) -> (# s', x #)
  r1 <- IO $ \s -> case casArray# arr 0# cur "b" s of (# s', f, x #) -> (# s', (I# f, x) #)
  r2 <- IO $ \s -> case casArray# arr 0# cur "c" s of (# s', f, x #) -> (# s', (I# f, x) #)
  print (r1, r2)
  ref <- newIORef (error "never forced" :: Int)
  _ <- atomicSwapIORef ref 5
  old <- atomicSwapIORef ref 6
  readIORef ref >>= \v -> print (old, v)
  -- A failed casArray# does not evaluate the element.
  MA arr2 <- IO $ \s -> case newArray# 1# (error "element") s of (# s', a #) -> (# s', MA a #)
  f <- IO $ \s -> case casArray# arr2 0# "x" "y" s of (# s', fl, _ #) -> (# s', I# fl #)
  print f
