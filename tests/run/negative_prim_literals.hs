{-# LANGUAGE MagicHash #-}
-- `-1#` is one token (happy's generated `happyDecodeAction -1# = ...`); `x-1#` still subtracts.
module Main where

import GHC.Exts

decode :: Int# -> String
decode 0# = "fail"
decode -1# = "accept"
decode n | isTrue# (n <# 0#) = "reduce " ++ show (I# (negateInt# (n +# 1#)))
         | otherwise = "shift " ++ show (I# (n -# 1#))

main :: IO ()
main = do
  putStrLn (decode 0#)
  putStrLn (decode -1#)
  putStrLn (decode -5#)
  putStrLn (decode 7#)
  let x = 10#
  print (I# (x -# 1#), D# (-2.5##), F# (-1.5#), W# (int2Word# -1#))
  print (I# (case -3# of { -3# -> 1#; _ -> 0# }), I# (2#+#3#), I# (1#::Int#))
