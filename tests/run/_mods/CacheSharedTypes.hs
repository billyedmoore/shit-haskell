{-# LANGUAGE RankNTypes #-}
module CacheSharedTypes where

data Box a = Box a deriving (Eq, Show)
type Closed = Box (Box (Box (Int, Bool)))

first, second :: Closed -> Closed
first x = x
second x = first x

poly :: (forall a. a -> a) -> Closed -> (Closed, Closed)
poly f x = (f (first x), f (second x))

class Label a where
  label :: a -> String
instance Label Closed where
  label _ = "closed"
