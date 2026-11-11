import Control.Concurrent
import Control.Concurrent.STM
import Control.Monad

main :: IO ()
main = do
  q <- newTQueueIO
  done <- newEmptyMVar
  total <- newTVarIO (0 :: Int)
  forM_ [1 .. 3 :: Int] $ \w -> forkIO $ do
    forM_ [1 .. 10] $ \i -> atomically (writeTQueue q (w * 100 + i))
    putMVar done ()
  replicateM_ 3 (takeMVar done)
  xs <- atomically (flushTQueue q)
  print (length xs, sum xs)
  tm <- newEmptyTMVarIO
  _ <- forkIO (atomically (putTMVar tm "hello"))
  v <- atomically (takeTMVar tm)
  putStrLn v
  ch <- newBroadcastTChanIO
  r1 <- atomically (dupTChan ch)
  atomically (mapM_ (writeTChan ch) [1, 2, 3 :: Int])
  ys <- atomically (replicateM 3 (readTChan r1))
  print ys
  forM_ [1 .. 100] $ \i -> atomically (modifyTVar' total (+ i))
  readTVarIO total >>= print
  res <- atomically ((readTVar total >>= \t -> check (t > 10000) >> return "big") `orElse` return "small")
  putStrLn res
