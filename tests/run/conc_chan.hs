import Control.Concurrent
import Control.Concurrent.Chan
import Control.Monad

main :: IO ()
main = do
  ch <- newChan
  results <- newChan
  -- Three workers square what they read; a Nothing ends a worker.
  forM_ [1 .. 3 :: Int] $ \w -> forkIO $
    let loop = do
          m <- readChan ch
          case m of
            Nothing -> writeChan results Nothing
            Just x -> writeChan results (Just (x * x :: Int)) >> loop
    in loop
  forM_ [1 .. 20] (writeChan ch . Just)
  replicateM_ 3 (writeChan ch Nothing)
  let collect 0 acc = return acc
      collect n acc = do
        r <- readChan results
        case r of
          Nothing -> collect (n - 1 :: Int) acc
          Just v -> collect n (acc + v)
  s <- collect 3 0
  print s
  -- getChanContents is lazy.
  c2 <- newChan
  _ <- forkIO (mapM_ (writeChan c2) [1 .. 5 :: Int])
  xs <- getChanContents c2
  print (take 5 xs)
