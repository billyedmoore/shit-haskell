{-# LANGUAGE TypeFamilies, DefaultSignatures, FlexibleContexts #-}
-- A given equality between a family application and a variable it mentions
-- (mono-traversable's `default omap :: (Functor f, Element (f a) ~ a, f a ~ mono) => ..`): it
-- rewrites the application to the variable, and does not refine the variable to it.
module Main where

type family Element mono
type instance Element [a] = a
type instance Element (Maybe a) = a

class MonoFunctor mono where
  omap :: (Element mono -> Element mono) -> mono -> mono
  default omap :: (Functor f, Element (f a) ~ a, f a ~ mono) => (Element mono -> Element mono) -> mono -> mono
  omap = fmap

instance MonoFunctor [a]
instance MonoFunctor (Maybe a)

newtype Text = Text String deriving Show
type instance Element Text = Char

instance MonoFunctor Text where
  omap f (Text s) = Text (map f s)

main :: IO ()
main = do
  print (omap (+ 1) [1, 2, 3 :: Int])
  print (omap not (Just True))
  print (omap succ (Text "HAL"))
