module Control.Monad.State (
    State, StateT, runState, evalState, execState, get, put, modify, modify', gets, state,
    runStateT, evalStateT, execStateT, lift, liftIO, MonadIO
  ) where

import Base
import Data.Functor.Identity

newtype StateT s m a = StateT (s -> m (a, s))

type State s = StateT s Identity

class MonadTrans t where
  lift :: Monad m => m a -> t m a

instance MonadTrans (StateT s) where
  lift m = StateT (\s -> m >>= \a -> return (a, s))

class Monad m => MonadIO m where
  liftIO :: IO a -> m a

instance MonadIO IO where
  liftIO m = m

instance MonadIO m => MonadIO (StateT s m) where
  liftIO m = lift (liftIO m)

runStateT :: StateT s m a -> s -> m (a, s)
runStateT (StateT f) s = f s

instance Monad m => Functor (StateT s m) where
  fmap f (StateT g) = StateT (\s -> g s >>= \(a, s') -> return (f a, s'))

instance Monad m => Applicative (StateT s m) where
  pure a = StateT (\s -> return (a, s))
  StateT mf <*> StateT mx = StateT (\s -> mf s >>= \(f, s1) -> mx s1 >>= \(x, s2) -> return (f x, s2))

instance Monad m => Monad (StateT s m) where
  StateT m >>= k = StateT (\s -> m s >>= \(a, s1) -> runStateT (k a) s1)

state :: Monad m => (s -> (a, s)) -> StateT s m a
state f = StateT (\s -> return (f s))

get :: Monad m => StateT s m s
get = StateT (\s -> return (s, s))

put :: Monad m => s -> StateT s m ()
put s = StateT (\_ -> return ((), s))

modify :: Monad m => (s -> s) -> StateT s m ()
modify f = StateT (\s -> return ((), f s))

modify' :: Monad m => (s -> s) -> StateT s m ()
modify' f = StateT (\s -> let s' = f s in s' `seq` return ((), s'))

gets :: Monad m => (s -> a) -> StateT s m a
gets f = StateT (\s -> return (f s, s))

runState :: State s a -> s -> (a, s)
runState m s = runIdentity (runStateT m s)

evalState :: State s a -> s -> a
evalState m s = fst (runState m s)

execState :: State s a -> s -> s
execState m s = snd (runState m s)

evalStateT :: Monad m => StateT s m a -> s -> m a
evalStateT m s = runStateT m s >>= \(a, _) -> return a

execStateT :: Monad m => StateT s m a -> s -> m s
execStateT m s = runStateT m s >>= \(_, s') -> return s'
