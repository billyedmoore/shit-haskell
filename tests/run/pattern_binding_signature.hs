-- A pattern binding whose pattern has a signature (cabal-install's `let (a, b) :: (T, U) = e`).
{-# LANGUAGE ScopedTypeVariables #-}

pairs :: [Int] -> ([Int], [Int])
pairs xs =
  let (evens, odds) :: ([Int], [Int]) = (filter even xs, filter odd xs)
      Just n :: Maybe Int = lookup 2 (zip xs xs)
  in (evens ++ [n], odds)

(top1, top2) :: (String, Bool) = ("top", True)

main :: IO ()
main = do
  print (pairs [1 .. 6])
  print (top1, top2)
  let [a, b] :: [Char] = "xy"
  print (b, a)
