{-# LANGUAGE DuplicateRecordFields #-}
module Inputs (PreConf (..), PostConf (..), Other (..), mkPre) where

data PreConf = PreConf { buildConfig :: String, verbosity :: Int } deriving (Show, Eq)
data PostConf = PostConf { buildConfig :: String, extra :: [String] } deriving Show
newtype Other = Other { onlyHere :: Bool } deriving Show

mkPre :: String -> PreConf
mkPre b = PreConf { buildConfig = b, verbosity = 1 }
