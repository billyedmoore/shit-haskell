{-# LANGUAGE MagicHash, UnboxedTuples #-}
-- Byte-array reads and writes computed inline (codegen bytes_fast_path) in a loop whose raw
-- parameters live in registers: the index is the loop's third raw argument.
import GHC.Exts
import GHC.IO (IO (..))

data MBA = MBA (MutableByteArray# RealWorld)

main :: IO ()
main = do
  MBA a <- IO $ \s -> case newByteArray# 800# s of (# s', a #) -> (# s', MBA a #)
  let fill :: Int# -> Int# -> Int# -> State# RealWorld -> State# RealWorld
      fill n step k s = case k ># n of
        1# -> s
        _ -> case writeIntArray# a k (k *# step) s of s' -> fill n step (k +# 1#) s'
      total :: Int# -> Int# -> Int# -> Int# -> State# RealWorld -> (# State# RealWorld, Int #)
      total n acc w k s = case k ># n of
        1# -> (# s, I# acc #)
        _ -> case readIntArray# a k s of
          (# s', v #) -> case readWord8Array# a (k *# 8#) s' of
            (# s'', b #) -> total n (acc +# v +# w +# word2Int# (word8ToWord# b)) w (k +# 1#) s''
  r <- IO $ \s -> case fill 99# 3# 0# s of s' -> total 99# 0# 7# 0# s'
  print r
