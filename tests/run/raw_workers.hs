{-# LANGUAGE MagicHash #-}
-- Functions whose strict Int#/Char# parameters are only used raw get workers taking them
-- unboxed: exact calls use the worker, other uses the ordinary entry.
import GHC.Exts

-- Recursive, with a raw result returned (boxed once).
countUp :: Int# -> Int# -> Int
countUp acc n = case n ># 0# of
  1# -> countUp (acc +# n) (n -# 1#)
  _ -> I# acc

-- A raw Char# parameter, returned as a Char.
nextLetter :: Char# -> Char
nextLetter c = case ord# c of
  122# -> C# 'a'#
  o -> C# (chr# (o +# 1#))

-- Mutual recursion between two workers.
isEven', isOdd' :: Int# -> Bool
isEven' n = case n of
  0# -> True
  _ -> isOdd' (n -# 1#)
isOdd' n = case n of
  0# -> False
  _ -> isEven' (n -# 1#)

-- Boxed Int arguments that are thunks: evaluated before the worker's call.
step :: Int -> Int -> Int
step (I# a) (I# b) = I# (a *# 10# +# b)

main :: IO ()
main = do
  print (countUp 0# 100000#, countUp 5# 0#)
  print (map (\(C# c) -> nextLetter c) "azby")
  print (isEven' 1001#, isOdd' 77#)
  print (foldl step 0 [1 .. 9], step (length [1, 2, 3]) (sum [4, 5]))
  -- Partial application and use as a value: the ordinary entry.
  let f = countUp 1#
      g = step 7
  print (f 10#, map g [1, 2, 3], zipWith step [1, 2] [3, 4])
