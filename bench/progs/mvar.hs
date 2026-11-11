import Control.Concurrent
import Control.Monad

-- Two threads passing a counter through MVars.
main :: IO ()
main = do
  a <- newEmptyMVar
  b <- newEmptyMVar
  let n = 20000 :: Int
  _ <- forkIO $ forM_ [1 .. n] $ \_ -> takeMVar a >>= putMVar b . (+ 1)
  r <- foldM (\acc _ -> putMVar a acc >> takeMVar b) 0 [1 .. n]
  print r
