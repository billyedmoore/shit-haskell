{-# LANGUAGE DeriveFunctor #-}
-- A derived Functor for a newtype over a function into a functor variable (ghc-lib-parser's
-- `newtype UniqDSMT m result = UDSMT' (DUniqSupply -> m (result, DUniqSupply)) deriving Functor`):
-- the instance needs `Functor m`.
module Main where

newtype StateT s m a = StateT (s -> m (a, s)) deriving (Functor)

newtype Reader r m a = Reader (r -> Int -> m a) deriving (Functor)

runStateT :: StateT s m a -> s -> m (a, s)
runStateT (StateT f) = f

runReader :: Reader r m a -> r -> m a
runReader (Reader f) r = f r 0

main :: IO ()
main = do
  r <- runStateT (fmap (+ 1) (StateT (\s -> return (s * 2, s + 1)))) (5 :: Int)
  print (r :: (Int, Int))
  print (runStateT (fmap show (StateT (\s -> Just (s, s)))) 'x')
  print (runReader (fmap length (Reader (\r _ -> [r, r]))) "abc")
