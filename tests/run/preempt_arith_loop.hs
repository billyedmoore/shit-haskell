{-# LANGUAGE BangPatterns #-}
-- A loop doing only arithmetic still lets other threads run and takes exceptions thrown to it:
-- a function's entry is a preemption point.
import Control.Concurrent
import Control.Exception
import System.Timeout (timeout)

spin :: Int -> Int -> Int
spin !acc n = if n == -1 then acc else spin ((acc + n * n) `mod` 1000003) (n + 1)
{-# NOINLINE spin #-}

main :: IO ()
main = do
  -- `timeout` throws to the looping thread.
  r <- timeout 200000 (evaluate (spin 0 0))
  print r
  -- Another thread runs while this one loops, and kills it.
  done <- newEmptyMVar
  t <- forkIO ((evaluate (spin 0 1) >>= print) `finally` putMVar done ())
  threadDelay 100000
  killThread t
  takeMVar done
  putStrLn "killed"
