-- Base's RULES fuse list producers and consumers: results, laziness (infinite lists, unforced
-- elements, short circuits) and sharing must be as without them.
import Data.Char (toUpper)
import Data.List (foldl')
import qualified Data.Foldable as F

main :: IO ()
main = do
  print (sum (map (\x -> x * x) [1 .. 1000 :: Int]))
  print (take 5 (map (* 3) [1 :: Int ..]), takeWhile (< 40) (filter odd (map (* 7) [1 :: Int ..])))
  print (and (map (< 10) [1 :: Int ..]), any (> 1000) [1 :: Int ..])
  print (length (map (\_ -> error "unforced" :: Int) [1 .. 10 :: Int]))
  print (take 3 [x | x <- [1 :: Int ..], even x, x `mod` 3 == 0])
  print (zip [1 :: Int ..] "abc", sum (zipWith (*) [1 .. 10] [10, 9 .. 1 :: Int]))
  print (concatMap (\x -> [x, x + 100]) [1 .. 4 :: Int], foldr (\x acc -> x : take 2 acc) [] [1 .. 10 :: Int])
  putStrLn ("lit" ++ map toUpper "eral" ++ show (length ([] ++ [1, 2 :: Int])))
  print (foldl (flip (:)) [] [1 .. 5 :: Int], foldr (&&) True (cycle [True, False]))
  let xs = map (* 2) [1 .. 6 :: Int]
  print (sum xs, product xs, reverse xs)
  print (elem 99 [1, 3 .. 101 :: Int], maximum (map negate [5, 3, 9 :: Int]), lookup 3 (zip [1 ..] "xyz"))
  -- Step functions given all three arguments (folds over small containers).
  print (foldl' (+) 1 (Just (41 :: Int)), F.foldl' (flip (:)) [] (Just 'x'), foldl (-) 10 (Right 3 :: Either String Int))
  print (F.foldr' (:) [] "abc", sum (Just (2 :: Int)), foldl' (\a b -> a * 2 + b) 0 [1, 0, 1, 1 :: Int])
