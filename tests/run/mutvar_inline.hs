-- Inline readMutVar#/writeMutVar#: lazy contents, strict and lazy modification, IORefs inside
-- data and behind thunks, and collections between writes.
import Control.Exception
import Control.Monad
import Data.IORef

data Counters = Counters { hits :: IORef Int, names :: IORef [String] }

main :: IO ()
main = do
  r <- newIORef (0 :: Int)
  forM_ [1 .. 200000] $ \i -> modifyIORef' r (+ i)
  readIORef r >>= print
  -- Lazy contents: a thunk is stored and only evaluated when read and demanded.
  lazyRef <- newIORef (error "never forced" :: Int)
  writeIORef lazyRef 7
  modifyIORef lazyRef (* 6)
  readIORef lazyRef >>= print
  boom <- newIORef (0 :: Int)
  writeIORef boom (error "forced late")
  v <- readIORef boom
  r2 <- try (evaluate v)
  putStrLn (either (\(ErrorCall m) -> m) show r2)
  -- IORefs inside a record, reached through a lazily built value.
  c <- Counters <$> newIORef 0 <*> newIORef []
  let pick = if length "abc" == 3 then c else error "unused"
  forM_ (words "alpha beta gamma delta") $ \w -> do
    modifyIORef' (hits pick) (+ 1)
    modifyIORef (names pick) (w :)
  (,) <$> readIORef (hits c) <*> readIORef (names c) >>= print
  -- Many references and allocation between writes.
  refs <- mapM newIORef [1 .. 50 :: Int]
  forM_ [1 .. 2000] $ \k -> forM_ refs $ \ref -> modifyIORef' ref (\x -> (x * 31 + k) `mod` 1000003)
  sum <$> mapM readIORef refs >>= print
