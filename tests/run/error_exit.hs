main :: IO ()
main = do
  putStrLn "before"
  _ <- return $! length [1 .. 10 :: Int]
  error "boom"
