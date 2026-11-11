import Control.Concurrent
import Control.Monad

-- Two threads hand a counter back and forth through a pair of MVars.
player :: String -> MVar Int -> MVar Int -> MVar () -> IO ()
player name inbox outbox done = loop
  where
    loop = do
      n <- takeMVar inbox
      if n >= 10
        then do
          putStrLn (name ++ " stops at " ++ show n)
          putMVar outbox (n + 1)
          putMVar done ()
        else do
          putStrLn (name ++ " got " ++ show n)
          putMVar outbox (n + 1)
          loop

main :: IO ()
main = do
  a <- newEmptyMVar
  b <- newEmptyMVar
  done <- newEmptyMVar
  _ <- forkIO (player "ping" a b done)
  _ <- forkIO (player "pong" b a done)
  putMVar a 0
  takeMVar done
  takeMVar done
  -- Many short-lived threads, each adding to a shared total.
  total <- newMVar (0 :: Int)
  finished <- newEmptyMVar
  forM_ [1 .. 100] $ \i -> forkIO $ do
    modifyMVar_ total (return . (+ i))
    putMVar finished ()
  replicateM_ 100 (takeMVar finished)
  readMVar total >>= print
