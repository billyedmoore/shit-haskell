{-# LANGUAGE ForeignFunctionInterface #-}
-- A forked process whose parent made safe foreign calls with other threads around: the child
-- starts with no worker threads, and its own safe calls still work with threads of its own.
import Control.Concurrent
import Control.Monad
import Foreign.C.Types
import System.Exit
import System.IO
import System.Posix.Process

foreign import ccall safe "unistd.h usleep" c_usleep :: CUInt -> IO CInt

main :: IO ()
main = do
  t <- forkIO (forever yield)
  _ <- c_usleep 1000
  killThread t
  hFlush stdout
  p <- forkProcess $ do
    d <- newEmptyMVar
    _ <- forkIO (putMVar d ())
    _ <- c_usleep 1000
    takeMVar d
    putStrLn "child done"
  st <- getProcessStatus True False p
  print st
  putStrLn "parent done"
