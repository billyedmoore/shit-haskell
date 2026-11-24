{-# LANGUAGE NoFlexibleInstances #-}
module RestrictedInstance where

class C a b
instance C (Maybe a) (Either a b)
