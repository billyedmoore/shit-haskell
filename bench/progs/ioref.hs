import Data.IORef
import Control.Monad

-- An IORef counter in an IO loop.
main :: IO ()
main = do
  r <- newIORef (0 :: Int)
  forM_ [1 .. 1000000] $ \i -> modifyIORef' r (+ i)
  readIORef r >>= print
