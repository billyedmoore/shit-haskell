import Control.Concurrent
import Control.Exception

main :: IO ()
main = do
  getMaskingState >>= print
  mask_ (getMaskingState >>= print)
  uninterruptibleMask_ (getMaskingState >>= print)
  mask $ \restore -> do
    getMaskingState >>= print
    restore (getMaskingState >>= print)
    getMaskingState >>= print
  getMaskingState >>= print
  -- A handler runs masked.
  (throwIO (ErrorCall "x") `catch` \e -> do
      putStrLn ("handler for " ++ show (e :: ErrorCall))
      getMaskingState >>= print)
  getMaskingState >>= print
  -- The state is restored when the masked action throws.
  r <- try (mask_ (throwIO (ErrorCall "inside"))) :: IO (Either ErrorCall ())
  print r
  getMaskingState >>= print
  -- A forked thread inherits the masking state.
  mv <- newEmptyMVar
  _ <- mask_ (forkIO (getMaskingState >>= putMVar mv))
  takeMVar mv >>= print
  _ <- forkIO (getMaskingState >>= putMVar mv)
  takeMVar mv >>= print
  -- An exception sent to a masked thread waits until it unmasks (or blocks).
  ready <- newEmptyMVar
  out <- newEmptyMVar
  t <- forkIO $ (mask_ (do putMVar ready (); let spin n = if n == (0 :: Int) then return () else spin (n - 1) in spin 100000; putMVar out "finished masked part") >> yield >> putMVar out "not reached")
          `catch` \e -> putMVar out ("then: " ++ show (e :: AsyncException))
  takeMVar ready
  killThread t
  takeMVar out >>= putStrLn
  takeMVar out >>= putStrLn
