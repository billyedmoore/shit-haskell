module Control.DeepSeq (NFData(..), deepseq, force, ($!!), (<$!!>)) where

import Base

infixr 0 $!!, `deepseq`

class NFData a where
  rnf :: a -> ()
  rnf x = x `seq` ()

instance NFData Int
instance NFData Integer
instance NFData Double
instance NFData Char
instance NFData Bool
instance NFData ()

instance NFData a => NFData [a] where
  rnf [] = ()
  rnf (x : xs) = rnf x `seq` rnf xs

instance NFData a => NFData (Maybe a) where
  rnf Nothing = ()
  rnf (Just x) = rnf x

instance (NFData a, NFData b) => NFData (a, b) where
  rnf (a, b) = rnf a `seq` rnf b

instance (NFData a, NFData b) => NFData (Either a b) where
  rnf (Left a) = rnf a
  rnf (Right b) = rnf b

deepseq :: NFData a => a -> b -> b
deepseq a b = rnf a `seq` b

force :: NFData a => a -> a
force x = x `deepseq` x

($!!) :: NFData a => (a -> b) -> a -> b
f $!! x = x `deepseq` f x

(<$!!>) :: (Monad m, NFData b) => (a -> b) -> m a -> m b
f <$!!> m = m >>= \x -> return $!! f x
