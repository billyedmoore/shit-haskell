main :: IO ()
main = mapM_ (putStrLn . (\case { 0 -> "zero"; _ -> "other" })) [0, 1 :: Int]
