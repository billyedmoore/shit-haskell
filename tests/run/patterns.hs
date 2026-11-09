data T = A Int | B String | C deriving Show

describe :: [T] -> String
describe [] = "empty"
describe [A n] | n > 0 = "one positive A"
describe (A _ : rest@(_ : _)) = "A then " ++ show (length rest)
describe (B "hi" : _) = "greeting"
describe (_ : _) = "other"

lazyPat :: (Int, Int) -> Int
lazyPat ~(_, _) = 42

firstTwo :: [a] -> Maybe (a, a)
firstTwo (x : y : _) = Just (x, y)
firstTwo _ = Nothing

main :: IO ()
main = do
  mapM_ (putStrLn . describe) [[], [A 3], [A (-1), C], [B "hi"], [C, C]]
  print (lazyPat undefined)
  print (firstTwo "abc", firstTwo [True])
  let (q, r) = 17 `divMod` (5 :: Int)
  print (q, r)
  print [x | Just x <- [Just 1, Nothing, Just (3 :: Int)]]
  print (case "hello" of ('h' : _ : rest) -> rest; _ -> "")
  print (case 2.5 :: Double of 2.5 -> "exact"; _ -> "no")
