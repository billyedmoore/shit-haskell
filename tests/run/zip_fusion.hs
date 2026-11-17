-- `zip` fused with the list on its left (GHC's "foldr2/left"): lengths, laziness and sharing
-- are as before, with enumerations, comprehensions and infinite lists on either side.
import Debug.Trace (trace)

main :: IO ()
main = do
  print (zip [1 :: Int ..] "abc", zip "abc" [1 :: Int ..])
  print (zip [1 .. 2 :: Int] "abcdef", zip [1 .. 9 :: Int] "ab")
  print (zip ([] :: [Int]) (undefined :: [Char]))
  print (take 2 (zip [1 :: Int ..] (1 : 2 : undefined :: [Int])))
  print (and [c /= d | (d, c) <- zip [1 :: Int ..] [5, 6, 7]], or [c == d | (d, c) <- zip [1 :: Int ..] [9, 2]])
  print (sum [d * c | (d, c) <- zip [1 :: Int ..] [10, 20, 30]])
  print (zipWith (+) [1 :: Int ..] [10, 20, 30], zipWith3 (\a b c -> a + b + c) [1 :: Int ..] [1, 2] [5, 5, 5])
  print (zip3 [1 :: Int ..] "xy" [True, False, True])
  print (length (zip [trace "left" (1 :: Int), 2, 3] (map (\x -> trace ("right " ++ show x) x) [1 :: Int, 2])))
  print (lookup 3 (zip [1 :: Int ..] "abcde"), [i | (i, 'l') <- zip [0 :: Int ..] "hello"])
  print (foldr (\(i, c) acc -> show i ++ [c] ++ acc) "" (zip [1 :: Int ..] "xyz"))
