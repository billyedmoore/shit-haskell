{-# LANGUAGE TypeFamilies #-}
module CacheTypes (Box(..), Wrapped, Payload, Describe(..)) where

data Box a = Box { getBox :: a } deriving Show
type Wrapped a = Box a
type family Payload a

class Describe a where
  describe :: a -> String
