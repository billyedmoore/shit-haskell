import Control.Concurrent
import Control.Monad

main :: IO ()
main = do
  box <- newEmptyMVar
  done <- newEmptyMVar
  forM_ [1 .. 5 :: Int] $ \i -> forkIO $ do
    putMVar box (i * 10)
  vals <- replicateM 5 (takeMVar box)
  print (sum vals)
  _ <- forkIO (threadDelay 1000 >> putMVar done "child finished")
  takeMVar done >>= putStrLn
  counter <- newMVar (0 :: Int)
  ws <- forM [1 .. 4 :: Int] $ \_ -> do
    fin <- newEmptyMVar
    _ <- forkIO $ do
      replicateM_ 100 (modifyMVar_ counter (return . (+ 1)))
      putMVar fin ()
    return fin
  mapM_ takeMVar ws
  readMVar counter >>= print
