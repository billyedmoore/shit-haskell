{-# LANGUAGE MagicHash, UnboxedTuples #-}
-- SmallArray# primitives (unordered-containers and primitive use them; hlint needs them): create,
-- read and write, freeze and thaw, copy, clone, shrink and its size, across a collection.
import GHC.Exts
import GHC.IO (IO (..))

data SA a = SA (SmallArray# a)
data SMA a = SMA (SmallMutableArray# RealWorld a)

newSA :: Int -> a -> IO (SMA a)
newSA (I# n) x = IO $ \s -> case newSmallArray# n x s of (# s', a #) -> (# s', SMA a #)

writeSA :: SMA a -> Int -> a -> IO ()
writeSA (SMA a) (I# i) x = IO $ \s -> (# writeSmallArray# a i x s, () #)

readSA :: SMA a -> Int -> IO a
readSA (SMA a) (I# i) = IO $ readSmallArray# a i

freezeSA :: SMA a -> IO (SA a)
freezeSA (SMA a) = IO $ \s -> case unsafeFreezeSmallArray# a s of (# s', f #) -> (# s', SA f #)

elemsSA :: SA a -> [a]
elemsSA (SA a) = [x | I# i <- [0 .. I# (sizeofSmallArray# a) - 1], let (# x #) = indexSmallArray# a i]

shrinkSA :: SMA a -> Int -> IO Int
shrinkSA (SMA a) (I# n) = IO $ \s -> case getSizeofSmallMutableArray# a (shrinkSmallMutableArray# a n s) of
  (# s', k #) -> (# s', I# k #)

main :: IO ()
main = do
  m <- newSA 10 (0 :: Int)
  mapM_ (\i -> writeSA m i (i * i)) [0 .. 9]
  readSA m 7 >>= print
  f@(SA fa) <- freezeSA m
  print (elemsSA f)
  print (elemsSA (SA (cloneSmallArray# fa 2# 3#)))
  m2 <- IO $ \s -> case thawSmallArray# fa 1# 4# s of (# s', a #) -> (# s', SMA a #)
  IO (\s -> (# copySmallArray# fa 5# (case m2 of SMA a -> a) 0# 2# s, () #))
  freezeSA m2 >>= print . elemsSA
  -- Many arrays across collections.
  arrs <- mapM (\k -> newSA 8 [k]) [1 .. 20000 :: Int]
  vs <- mapM (\a -> readSA a 3) arrs
  print (sum (concat vs))
  shrinkSA m 4 >>= print
  freezeSA m >>= print . elemsSA
