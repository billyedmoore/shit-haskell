{-# LANGUAGE OverloadedStrings #-}
-- Typeable evidence in a module with OverloadedStrings: the names it carries are Strings, not
-- overloaded literals (Cabal-syntax's VersionRange, whose class has Typeable as a superclass).
module Main where

import Data.String (IsString (..))
import Data.Typeable (Typeable, typeRep, typeOf, Proxy (..))

class Typeable a => Named a where
  name :: a -> String
  name x = show (typeOf x)

data Range = Any | Later Int deriving Show

instance Named Range
instance Named Bool

newtype Label = Label String deriving Show
instance IsString Label where fromString = Label

main :: IO ()
main = do
  putStrLn (name (Later 3))
  putStrLn (name True)
  print (typeRep (Proxy :: Proxy (Maybe Range)), "lbl" :: Label)
