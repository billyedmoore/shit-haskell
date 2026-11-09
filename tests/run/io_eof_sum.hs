import System.IO

main :: IO ()
main = do
  hSetBuffering stdout NoBuffering
  eof <- isEOF
  if eof then putStrLn "no input" else go 0
  where
    go :: Int -> IO ()
    go acc = do
      done <- isEOF
      if done
        then putStrLn ("total " ++ show acc)
        else do
          l <- getLine
          go (acc + sum (map read (words l)))
