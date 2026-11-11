import Control.Concurrent
import Control.Exception
import Control.Monad
import Data.IORef
import System.IO.Error
import System.Posix

main :: IO ()
main = do
  -- A write to a pipe whose reading end is closed fails with EPIPE (SIGPIPE does not end the
  -- program).
  (rd, wr) <- createPipe
  closeFd rd
  r <- try (fdWrite wr "data") :: IO (Either IOException ByteCount)
  putStrLn (either (\e -> "write failed: " ++ show (ioeGetErrorType e)) show r)
  -- An interruptible call (sem_wait) is woken by killThread.
  sem <- semOpen "/hsc-test-interruptible" OpenSemFlags {semCreate = True, semExclusive = False} stdFileMode 0
  semUnlink "/hsc-test-interruptible"
  ref <- newIORef "not set"
  done <- newEmptyMVar
  tid <- forkIO $ (semWaitInterruptible sem >>= \b -> writeIORef ref (show b)) `finally` putMVar done ()
  threadDelay 100000
  killThread tid
  takeMVar done
  readIORef ref >>= putStrLn
  -- And it returns when the semaphore is posted.
  tid2 <- forkIO $ (semWaitInterruptible sem >>= \b -> writeIORef ref (show b)) `finally` putMVar done ()
  threadDelay 100000
  semPost sem
  takeMVar done
  readIORef ref >>= putStrLn
  _ <- return tid2
  putStrLn "end"
