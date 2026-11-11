import Control.Concurrent
import Control.Exception

main :: IO ()
main = do
  mv <- newEmptyMVar :: IO (MVar Int)
  r <- try (takeMVar mv) :: IO (Either BlockedIndefinitelyOnMVar Int)
  print r
  putStrLn "after"
  -- Uncaught: reported on stderr, exit code 1.
  v <- takeMVar mv
  print v
