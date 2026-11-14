module Rep (Scaled (..), scaledThing) where

data Scaled a = Scaled Int a

scaledThing :: Scaled a -> a
scaledThing (Scaled _ a) = a
