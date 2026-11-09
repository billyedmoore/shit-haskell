main :: IO ()
main = do
  name <- getLine
  putStrLn ("Hello, " ++ name ++ "!")
  n <- readLn :: IO Int
  xs <- mapM (const readLn) [1 .. n] :: IO [Int]
  print (sum xs)
  rest <- getContents
  mapM_ (putStrLn . reverse) (lines rest)
