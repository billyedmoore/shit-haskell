import Data.Char (toUpper)

-- Text processing on String: words, map, concat, length.
main :: IO ()
main = do
  let text = concat (replicate 20000 "the quick brown fox jumps over the lazy dog ")
      ws = words text
      long = filter ((> 3) . length) ws
  print (length ws, length long, length (unwords (map (map toUpper) long)))
