module Data.Function (id, const, (.), flip, ($), (&), on, fix) where

import Base

infixl 0 `on`
infixl 1 &

on :: (b -> b -> c) -> (a -> b) -> a -> a -> c
on f g x y = f (g x) (g y)

(&) :: a -> (a -> b) -> b
x & f = f x

fix :: (a -> a) -> a
fix f = let x = f x in x
