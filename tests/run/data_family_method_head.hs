-- A data family application against a class method's `v s a` (vector's MVector instance for
-- Complex): the unknown constructor is the family, not the instance's representation.
{-# LANGUAGE TypeFamilies, MultiParamTypeClasses, FlexibleInstances #-}
import Control.Monad.ST
import Data.Complex
import Data.STRef

data family MV s a
newtype instance MV s Int = MV_Int (STRef s Int)
data instance MV s (a, b) = MV_2 (MV s a) (MV s b)
newtype instance MV s (Complex a) = MV_Complex (MV s (a, a))

class M v a where
  rep :: a -> ST s (v s a)
  get :: v s a -> ST s a

instance M MV Int where
  rep x = MV_Int <$> newSTRef x
  get (MV_Int r) = readSTRef r

instance (M MV a, M MV b) => M MV (a, b) where
  rep (x, y) = MV_2 <$> rep x <*> rep y
  get (MV_2 a b) = (,) <$> get a <*> get b

instance M MV a => M MV (Complex a) where
  rep (x :+ y) = MV_Complex <$> rep (x, y)
  get (MV_Complex v) = uncurry (:+) <$> get v

asMV :: MV s a -> MV s a
asMV = id

main :: IO ()
main = do
  print (runST (rep (3 :+ 4 :: Complex Int) >>= get . asMV))
  print (runST (rep ((1, 2) :+ (3, 4) :: Complex (Int, Int)) >>= get . asMV))
