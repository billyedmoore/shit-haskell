module Control.Monad (
    Functor(fmap), Monad((>>=), (>>), return), MonadFail(fail), Applicative(pure, (<*>), (*>), (<*)),
    mapM, mapM_, forM, forM_, sequence, sequence_, (=<<), (>=>), (<=<), forever,
    void, join, when, unless, liftM, liftM2, ap, replicateM, replicateM_,
    foldM, foldM_, zipWithM, zipWithM_, filterM, (<$!>), (<$>)
  ) where

import Base

infixr 1 >=>, <=<
infixl 4 <$!>

forM :: Monad m => [a] -> (a -> m b) -> m [b]
forM xs f = mapM f xs

forM_ :: Monad m => [a] -> (a -> m b) -> m ()
forM_ xs f = mapM_ f xs

(>=>) :: Monad m => (a -> m b) -> (b -> m c) -> a -> m c
f >=> g = \x -> f x >>= g

(<=<) :: Monad m => (b -> m c) -> (a -> m b) -> a -> m c
g <=< f = \x -> f x >>= g

forever :: Monad m => m a -> m b
forever a = a >> forever a

void :: Functor f => f a -> f ()
void x = fmap (\_ -> ()) x

join :: Monad m => m (m a) -> m a
join x = x >>= id

when :: Applicative f => Bool -> f () -> f ()
when p s = if p then s else pure ()

unless :: Applicative f => Bool -> f () -> f ()
unless p s = if p then pure () else s

liftM :: Monad m => (a -> b) -> m a -> m b
liftM f m = m >>= \x -> return (f x)

liftM2 :: Monad m => (a -> b -> c) -> m a -> m b -> m c
liftM2 f a b = a >>= \x -> b >>= \y -> return (f x y)

ap :: Monad m => m (a -> b) -> m a -> m b
ap mf mx = mf >>= \f -> mx >>= \x -> return (f x)

replicateM :: Applicative m => Int -> m a -> m [a]
replicateM n x = if n <= 0 then pure [] else liftA2 (:) x (replicateM (n - 1) x)

replicateM_ :: Applicative m => Int -> m a -> m ()
replicateM_ n x = if n <= 0 then pure () else x *> replicateM_ (n - 1) x

foldM :: Monad m => (b -> a -> m b) -> b -> [a] -> m b
foldM _ z [] = return z
foldM f z (x : xs) = f z x >>= \z' -> foldM f z' xs

foldM_ :: Monad m => (b -> a -> m b) -> b -> [a] -> m ()
foldM_ f z xs = foldM f z xs >> return ()

zipWithM :: Applicative m => (a -> b -> m c) -> [a] -> [b] -> m [c]
zipWithM f xs ys = sequenceA (zipWith f xs ys)

zipWithM_ :: Applicative m => (a -> b -> m c) -> [a] -> [b] -> m ()
zipWithM_ f xs ys = foldr (*>) (pure ()) (zipWith f xs ys)

filterM :: Applicative m => (a -> m Bool) -> [a] -> m [a]
filterM p = foldr (\x -> liftA2 (\flg -> if flg then (x :) else id) (p x)) (pure [])

(<$!>) :: Monad m => (a -> b) -> m a -> m b
f <$!> m = m >>= \x -> let z = f x in z `seq` return z
