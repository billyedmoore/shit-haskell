classify :: Int -> String
classify n
  | n < 0 = "negative"
  | n == 0 = "zero"
  | isSmall = "small"
  | otherwise = "large"
  where
    isSmall = n < 10

collatz :: Int -> Int
collatz 1 = 0
collatz n
  | even n = 1 + collatz (n `div` 2)
  | otherwise = 1 + collatz (3 * n + 1)

main :: IO ()
main = do
  mapM_ (putStrLn . classify) [-5, 0, 7, 100]
  print (map collatz [1 .. 10])
  print (case compare 3 (4 :: Int) of
           LT -> "lt"
           EQ -> "eq"
           GT -> "gt")
  let x = 5 :: Int
  if x > 3 then putStrLn "big" else putStrLn "small"
