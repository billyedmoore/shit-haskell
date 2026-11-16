{-# LANGUAGE MagicHash #-}
-- Intermediate Int#/Word#/Char# results kept unboxed in raw frame slots: chained arithmetic,
-- literal cases on raw values, raw operands boxed for the runtime's slow path (thunk
-- operands), and collections while raw values are live.
import Control.Exception
import Data.Char (isSpace, toUpper)
import GHC.Exts

poly :: Int -> Int
poly (I# x) = I# ((x *# x) +# (3# *# x) -# 7#)

classify :: Int -> String
classify (I# x) = case x `remInt#` 4# of
  0# -> "zero"
  1# -> "one"
  _ -> "other"

-- The operand is a thunk the first time: the runtime's path takes the boxed raw value.
lazySum :: Int -> Int -> Int
lazySum a b = let c = a * 2 in c + b + c `quot` 3

-- Allocation (a list per step) while raw intermediates are live.
steps :: Int -> [Int]
steps n = [I# (y *# 2# +# 1#) | I# y <- [0 .. n], isTrue# (remInt# y 3# ==# 0#)]

wordStep :: Word -> Word
wordStep (W# w) = W# ((w `timesWord#` 3##) `plusWord#` 1##)

shout :: Char -> Char
shout (C# c) = case ord# c of
  n | isTrue# (n ># 96#) && isTrue# (n <# 123#) -> C# (chr# (n -# 32#))
  _ -> C# c

main :: IO ()
main = do
  print (map poly [-3, 0, 5, 100000])
  print (map classify [0, 5, 10, -3])
  print (lazySum 7 (sum [1 .. 10]))
  print (sum (steps 30000), length (steps 30000))
  print (map wordStep [0, maxBound, 12345])
  putStrLn (map shout "hello, World!") >> putStrLn (map toUpper "ok")
  print (length (filter (not . isSpace) (concat (replicate 2000 "a b\tc\n"))))
  r <- try (evaluate (5 `quot` length ([] :: [Int])))
  putStrLn (either (\e -> show (e :: ArithException)) show r)
