module BootAddedDefault where

class Select a where
  select :: a -> a
  select = id
