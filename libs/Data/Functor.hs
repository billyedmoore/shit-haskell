module Data.Functor (Functor(..), (<$>), ($>), (<&>), void) where

import Base

infixl 1 <&>
infixl 4 $>

($>) :: Functor f => f a -> b -> f b
m $> x = fmap (const x) m

(<&>) :: Functor f => f a -> (a -> b) -> f b
m <&> f = fmap f m

void :: Functor f => f a -> f ()
void x = fmap (const ()) x
