import Data.List (foldl')

sumStrict :: [Int] -> Int
sumStrict = go 0
  where
    go !acc [] = acc
    go !acc (x : xs) = go (acc + x) xs

main :: IO ()
main = do
  print (sumStrict [1 .. 100000])
  print (seq (1 :: Int) "seq ok")
  let x = undefined :: Int
  print (fst (3 :: Int, x `seq` ()) `seq` "not forced")
  print (foldl' max 0 [3, 9, 2 :: Int])
