import Control.Concurrent
import Control.Exception
import Control.Monad
import GHC.Conc

transfer :: TVar Int -> TVar Int -> Int -> STM ()
transfer from to n = do
  a <- readTVar from
  when (a < n) retry
  writeTVar from (a - n)
  b <- readTVar to
  writeTVar to (b + n)

main :: IO ()
main = do
  x <- newTVarIO 100
  y <- newTVarIO 0
  atomically (transfer x y 30)
  (,) <$> readTVarIO x <*> readTVarIO y >>= print
  -- orElse: the first alternative retries, the second runs.
  r <- atomically ((transfer x y 1000 >> return "first") `orElse` return "second")
  putStrLn r
  -- A transaction that throws leaves no trace of its writes.
  e <- try (atomically (writeTVar x 0 >> throwSTM (ErrorCall "abort"))) :: IO (Either ErrorCall ())
  print e
  readTVarIO x >>= print
  -- catchSTM undoes the writes of the failed part.
  atomically ((writeTVar y 999 >> throwSTM (ErrorCall "inner")) `catchSTM` \ex -> let _ = (ex :: ErrorCall) in writeTVar x 1)
  (,) <$> readTVarIO x <*> readTVarIO y >>= print
  -- A thread waits (retry) until another makes the condition true.
  gate <- newTVarIO False
  done <- newEmptyMVar
  _ <- forkIO $ do
    atomically (readTVar gate >>= \g -> unless g retry)
    putMVar done "gate opened"
  yield
  atomically (writeTVar gate True)
  takeMVar done >>= putStrLn
  -- Many threads increment a counter.
  c <- newTVarIO (0 :: Int)
  fin <- newEmptyMVar
  forM_ [1 .. 50 :: Int] $ \_ -> forkIO $ do
    replicateM_ 20 (atomically (readTVar c >>= writeTVar c . (+ 1)))
    putMVar fin ()
  replicateM_ 50 (takeMVar fin)
  readTVarIO c >>= print
