-- `T (..)` exports what of T is in scope, however it was imported: here a class and a method
-- imported separately (`import M (C, m)`), and a type with one field but no constructors.
module Main where

import Re

newtype Id a = Id { runId :: a }
instance Functor Id where fmap f (Id a) = Id (f a)
instance Applicative Id where
  pure = Id
  Id f <*> Id a = Id (f a)
instance Monad Id where Id a >>= f = f a
instance Throw Id where throwIt = error

main :: IO ()
main = do
  print (runId (return (3 :: Int)), runId (throwIt "no" >> return ()) `seq` ())
  print (map unBox [])
