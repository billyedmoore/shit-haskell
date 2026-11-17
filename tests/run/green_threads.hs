{-# LANGUAGE ForeignFunctionInterface #-}
-- Many threads at once, deep recursion inside forked threads, threads finishing in any order,
-- and safe foreign calls (with errno) while other threads keep running.
import Control.Concurrent
import Control.Exception
import Control.Monad
import Foreign.C.Error (Errno (..), getErrno)
import Foreign.C.Types

foreign import ccall safe "unistd.h usleep" c_usleep :: CUInt -> IO CInt
foreign import ccall safe "unistd.h close" c_close :: CInt -> IO CInt

-- Not tail recursive: the thread's stack grows.
deep :: Int -> Int
deep 0 = 0
deep n = 1 + deep (n - 1)

main :: IO ()
main = do
  -- A thousand threads, each handing its number back through one MVar.
  box <- newEmptyMVar
  forM_ [1 .. 1000 :: Int] $ \i -> forkIO (putMVar box i)
  xs <- replicateM 1000 (takeMVar box)
  print (sum xs, length xs)
  -- Deep recursion in forked threads.
  rs <- forM [1 .. 4 :: Int] $ \k -> do
    v <- newEmptyMVar
    _ <- forkIO (evaluate (deep (100000 * k)) >>= putMVar v)
    return v
  mapM takeMVar rs >>= print
  -- A safe foreign call that sleeps while another thread counts.
  counter <- newMVar (0 :: Int)
  done <- newEmptyMVar
  _ <- forkIO $ do
    replicateM_ 1000 (modifyMVar_ counter (return . (+ 1)) >> yield)
    putMVar done ()
  _ <- c_usleep 50000
  takeMVar done
  readMVar counter >>= print
  -- errno from a failing safe call, with another thread around.
  t <- forkIO (forever yield)
  r <- c_close (-1)
  Errno e <- getErrno
  killThread t
  print (r, e)
  -- An exception thrown to a thread blocked in an MVar, and one killed while it runs.
  m <- newEmptyMVar :: IO (MVar ())
  res <- newEmptyMVar
  w <- forkIO (takeMVar m `catch` \ThreadKilled -> putMVar res "killed while blocked")
  yield
  killThread w
  takeMVar res >>= putStrLn
