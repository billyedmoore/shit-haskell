{-# LANGUAGE TypeFamilies, FlexibleInstances #-}
module CacheInstances (wrap, unwrap) where

import CacheTypes

type instance Payload (Box a) = a
instance Show a => Describe (Box a) where
  describe b = "boxed " ++ show (getBox b)

wrap :: a -> Wrapped a
wrap = Box

unwrap :: Box a -> Payload (Box a)
unwrap = getBox
