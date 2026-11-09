module Control.Applicative (
    Applicative(..), Alternative(..), Functor(..), (<$>), (<$), (<**>), liftA, liftA2, liftA3,
    optional, asum
  ) where

import Base

infixl 3 <|>
infixl 4 <**>

class Applicative f => Alternative f where
  empty :: f a
  (<|>) :: f a -> f a -> f a
  some :: f a -> f [a]
  many :: f a -> f [a]
  some v = liftA2 (:) v (many v)
  many v = some v <|> pure []

instance Alternative Maybe where
  empty = Nothing
  Nothing <|> r = r
  l <|> _ = l

instance Alternative [] where
  empty = []
  (<|>) = (++)

(<**>) :: Applicative f => f a -> f (a -> b) -> f b
a <**> f = liftA2 (\x g -> g x) a f

liftA :: Applicative f => (a -> b) -> f a -> f b
liftA f a = fmap f a

liftA3 :: Applicative f => (a -> b -> c -> d) -> f a -> f b -> f c -> f d
liftA3 f a b c = liftA2 f a b <*> c

optional :: Alternative f => f a -> f (Maybe a)
optional v = fmap Just v <|> pure Nothing

asum :: Alternative f => [f a] -> f a
asum = foldr (<|>) empty
