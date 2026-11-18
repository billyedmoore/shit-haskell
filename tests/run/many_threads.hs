-- Many threads alive at once (each blocked on one MVar), then all handing back their numbers:
-- a thread's stacks are only reserved, so thousands cost little.
import Control.Concurrent
import Control.Monad

main :: IO ()
main = do
  box <- newEmptyMVar
  forM_ [1 .. 5000 :: Int] $ \i -> forkIO (putMVar box i)
  xs <- replicateM 5000 (takeMVar box)
  print (sum xs, length xs)
