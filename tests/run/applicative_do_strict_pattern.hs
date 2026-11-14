{-# LANGUAGE ApplicativeDo #-}
-- ApplicativeDo leaves a statement with a strict pattern to `>>=` (as GHC does), so the
-- pattern is matched: a failing match is not lost.
module Main where

import Control.Exception

main :: IO ()
main = do
  r <- try (do
    (x, y) <- return (throw (ErrorCall "matched") :: (Int, Int))
    pure ())
  print (either (\(ErrorCall m) -> m) (const "not matched") r)
  ~(a, b) <- return (undefined :: (Int, Int))
  putStrLn "lazy pattern not forced"
