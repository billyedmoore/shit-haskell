{-# LANGUAGE FlexibleContexts #-}
{-# LANGUAGE FlexibleInstances #-}
{-# LANGUAGE FunctionalDependencies #-}
{-# LANGUAGE MultiParamTypeClasses #-}
{-# LANGUAGE RankNTypes #-}
{-# LANGUAGE TypeFamilies #-}
{-# LANGUAGE UndecidableInstances #-}
module Main where

import Data.Functor.Identity

class Monad m => MonadBaseControl b m | m -> b where
  type StM m a
  liftBaseWith :: ((forall a. m a -> b (StM m a)) -> b r) -> m r

instance MonadBaseControl IO IO where
  type StM IO a = a
  liftBaseWith f = f id

newtype Wrap m a = Wrap { runWrap :: m a }

instance Functor m => Functor (Wrap m) where
  fmap f (Wrap m) = Wrap (fmap f m)
instance Applicative m => Applicative (Wrap m) where
  pure = Wrap . pure
  Wrap f <*> Wrap x = Wrap (f <*> x)
instance Monad m => Monad (Wrap m) where
  Wrap m >>= k = Wrap (m >>= runWrap . k)

type family StT (t :: (* -> *) -> * -> *) a
type instance StT Wrap a = a
type ComposeSt t m a = StM m (StT t a)

defaultLiftBaseWith :: MonadBaseControl b m => ((forall a. Wrap m a -> b (ComposeSt Wrap m a)) -> b r) -> Wrap m r
defaultLiftBaseWith f = Wrap (liftBaseWith (\runInBase -> f (runInBase . runWrap)))

instance MonadBaseControl b m => MonadBaseControl b (Wrap m) where
  type StM (Wrap m) a = ComposeSt Wrap m a
  liftBaseWith = defaultLiftBaseWith

main :: IO ()
main = do
  x <- runWrap (liftBaseWith (\run -> run (pure (99 :: Int) :: Wrap IO Int)) :: Wrap IO Int)
  print x
