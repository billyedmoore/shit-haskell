-- Word frequencies with Data.Map over generated text, then the most common words: a
-- containers-heavy workload with String keys and sorting.
import qualified Data.Map.Strict as M
import Data.List (sortBy)
import Data.Ord (comparing, Down (..))

wordsOf :: Int -> [String]
wordsOf n = [syllables (i * 7919 `mod` 5003) | i <- [1 .. n]]
  where
    syllables k = concatMap pick [k `mod` 11, (k `div` 11) `mod` 13, (k `div` 143) `mod` 5]
    pick j = ["ka", "lo", "mi", "ne", "su", "ta", "ri", "po", "de", "fu", "gi", "ho", "ja"] !! j

main :: IO ()
main = do
  let freq = M.fromListWith (+) [(w, 1 :: Int) | w <- wordsOf 300000]
      top = take 5 (sortBy (comparing (Down . snd)) (M.toList freq))
  print (M.size freq)
  mapM_ print top
