-- forM_/mapM_ over enumerations and comprehensions fuse into loops that take the state token:
-- effects run once each, in order, and an exception stops the loop where it is raised.
import Control.Exception
import Control.Monad
import Data.Foldable (for_, traverse_)
import Data.IORef

main :: IO ()
main = do
  r <- newIORef (0 :: Int)
  forM_ [1 .. 100000] $ \i -> modifyIORef' r (+ i)
  readIORef r >>= print
  forM_ [1 .. 5 :: Int] print
  mapM_ (\x -> when (odd x) (print x)) [x * x | x <- [1 .. 6 :: Int]]
  for_ [10, 20 .. 50 :: Int] (\x -> modifyIORef r (subtract x))
  readIORef r >>= print
  traverse_ print "ab"
  sequence_ [putStr (show i ++ " ") | i <- [1 .. 3 :: Int]] >> putStrLn ""
  res <- try (forM_ [1 :: Int ..] $ \i -> do
    when (i == 4) (throwIO (userError "stop"))
    print i)
  print (res :: Either IOException ())
  let act = forM_ [1 .. 3 :: Int] (\i -> modifyIORef r (+ i))
  act >> act
  readIORef r >>= print
  n <- newIORef (0 :: Int)
  forM_ [1 .. 3 :: Int] $ \i -> forM_ [1 .. i] $ \j -> modifyIORef n (+ (i * j))
  readIORef n >>= print
