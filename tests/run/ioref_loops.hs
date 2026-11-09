import Data.IORef
import Control.Monad

main :: IO ()
main = do
  ref <- newIORef (0 :: Int)
  forM_ [1 .. 10] $ \i ->
    when (even i) $ modifyIORef' ref (+ i)
  readIORef ref >>= print
  unless False $ putStrLn "unless works"
  xs <- forM [1 .. 3 :: Int] (\i -> return (i * i))
  print xs
  r <- foldM (\acc x -> do { print x; return (acc + x) }) 0 [7, 8 :: Int]
  print r
  replicateM_ 2 (putStr "hi ")
  putStrLn ""
  print =<< atomicModifyIORef' ref (\v -> (v * 2, v + 1))
  print (zipWithM_ (\_ _ -> Nothing) [1 :: Int] [2 :: Int])
