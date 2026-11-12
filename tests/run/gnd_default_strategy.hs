{-# LANGUAGE GeneralizedNewtypeDeriving #-}
-- With GeneralizedNewtypeDeriving a newtype derives Functor (and Eq, Ord, Enum, ...) through its
-- representation, Show and Read the stock way (Cabal's CheckM over ReaderT and WriterT).
module Main where

import Control.Monad.Reader (ReaderT, ask, runReaderT)
import Control.Monad.Writer (Writer, tell, runWriter)

newtype CheckM a = CheckM (ReaderT Int (Writer [String]) a)
  deriving (Functor, Applicative, Monad)

newtype Score = Score Int deriving (Eq, Ord, Show, Enum, Bounded, Num)

warn :: String -> CheckM ()
warn s = CheckM (tell [s])

limit :: CheckM Int
limit = CheckM ask

check :: [Int] -> CheckM Int
check xs = do
  l <- limit
  mapM_ (\x -> if x > l then warn ("too big: " ++ show x) else return ()) xs
  fmap length (return xs)

run :: CheckM a -> Int -> (a, [String])
run (CheckM m) l = runWriter (runReaderT m l)

main :: IO ()
main = do
  print (run (check [1, 5, 10, 3]) 4)
  print (Score 3 + Score 4, [Score 1 .. Score 3], Score 2 < Score 5, succ (Score 9))
