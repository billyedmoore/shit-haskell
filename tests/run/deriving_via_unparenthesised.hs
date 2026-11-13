{-# LANGUAGE DerivingVia, StandaloneDeriving #-}
-- A standalone deriving-via type written without parentheses (aeson's
-- `deriving via Maybe a instance FromJSON a => FromJSON (First a)`).
module Main where

import qualified Data.Monoid as Monoid

class Describe a where
  describe :: a -> String

instance Describe a => Describe (Maybe a) where
  describe Nothing = "none"
  describe (Just x) = "just " ++ describe x

instance Describe Int where
  describe = show

deriving via Maybe a instance Describe a => Describe (Monoid.First a)

main :: IO ()
main = mapM_ (putStrLn . describe) [Monoid.First (Just (3 :: Int)), Monoid.First Nothing]
