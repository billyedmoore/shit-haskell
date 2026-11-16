-- A pattern-match failure constrains no strictness (it is bottom), but a parameter the
-- returning alternatives do not use stays lazy, and the failure is still what is reported.
import Control.Exception

firstOr :: Maybe Int -> Int -> Int
firstOr (Just x) _ = x

addJust :: Maybe Int -> Int -> Int
addJust (Just x) y = x + y
{-# NOINLINE addJust #-}

count :: [a] -> Int -> Int
count [] n = n
count (_ : ys) n = count ys (n + 1)

main :: IO ()
main = do
  print (firstOr (Just 1) undefined)
  print (addJust (Just 2) 3, count (replicate 200000 'x') 0)
  r <- try (evaluate (firstOr Nothing 5))
  putStrLn (either (\e -> takeWhile (/= '\n') (show (e :: SomeException))) show r)
  r2 <- try (evaluate (addJust Nothing 4))
  putStrLn (either (\e -> takeWhile (/= '\n') (show (e :: SomeException))) show r2)
