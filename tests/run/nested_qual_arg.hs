{-# LANGUAGE ConstraintKinds #-}
{-# LANGUAGE FunctionalDependencies #-}
{-# LANGUAGE MultiParamTypeClasses #-}
{-# LANGUAGE RankNTypes #-}
{-# LANGUAGE ScopedTypeVariables #-}
{-# LANGUAGE TypeFamilies #-}
module Main where

import Data.Kind (Constraint)

type family Extra (k :: *) (p :: * -> *) :: Constraint
type instance Extra Int p = Show (p Int)

type Inner k p a = Extra k p => p a -> String

newtype Box k a = Box (forall p. Functor p => Inner k p a)

unBox :: Functor p => Box k a -> Inner k p a
unBox (Box f) = f

rewrap :: forall k a. Box k a -> Box k a
rewrap (Box o) = Box (go o)
  where
    go :: forall p. Inner k p a -> Inner k p a
    go f = f

main :: IO ()
main = do
  let b :: Box Int Int
      b = Box (\p -> show (fmap (+1) p))
      b' = rewrap b
  putStrLn (unBox b' (Just 41))
