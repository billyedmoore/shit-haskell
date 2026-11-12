module Conf (Config (..), Inner (..)) where

data Inner = Inner { depth :: Int, label :: String } deriving Show
data Config = Config { inner :: Inner, verbose :: Bool } deriving Show
