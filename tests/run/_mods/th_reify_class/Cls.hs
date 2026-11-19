{-# LANGUAGE FunctionalDependencies #-}
module Cls (Pretty (..), Convert (..), Box (..)) where

class Show a => Pretty a where
  pretty :: a -> String
  prettyList :: [a] -> String
  prettyList = concatMap pretty

class Convert a b | a -> b where
  convert :: a -> b

data Box a = Box a

instance Pretty Int where pretty = show
instance Pretty Bool where pretty b = if b then "yes" else "no"
instance Pretty a => Pretty [a] where pretty = prettyList
instance Convert Int Integer where convert = fromIntegral
