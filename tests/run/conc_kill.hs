import Control.Concurrent
import Control.Exception
import Control.Monad

main :: IO ()
main = do
  blocker <- newEmptyMVar :: IO (MVar ())
  report <- newEmptyMVar
  t <- forkIO $
    (takeMVar blocker >> putMVar report "not killed")
      `catch` \e -> putMVar report ("caught: " ++ show (e :: AsyncException))
  yield
  killThread t
  takeMVar report >>= putStrLn
  -- finally runs when a thread is killed.
  cleaned <- newEmptyMVar
  started <- newEmptyMVar
  t2 <- forkIO $ (putMVar started () >> forever (threadDelay 1000)) `finally` putMVar cleaned "cleanup ran"
  takeMVar started
  killThread t2
  takeMVar cleaned >>= putStrLn
  -- throwTo with a custom exception.
  got <- newEmptyMVar
  ready <- newEmptyMVar
  t3 <- forkIO $ handle (\e -> putMVar got (show (e :: ErrorCall))) $ do
    putMVar ready ()
    forever yield
  takeMVar ready
  throwTo t3 (ErrorCall "stop now")
  takeMVar got >>= putStrLn
  -- An exception thrown to oneself is synchronous.
  me <- myThreadId
  r <- try (throwTo me (ErrorCall "self")) :: IO (Either ErrorCall ())
  print r
  putStrLn "done"
