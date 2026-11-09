module Data.Complex (Complex(..), realPart, imagPart, magnitude, phase, conjugate, mkPolar, cis) where

import Base

infix 6 :+

data Complex a = a :+ a

instance Eq a => Eq (Complex a) where
  (a :+ b) == (c :+ d) = a == c && b == d

instance Show a => Show (Complex a) where
  showsPrec d (a :+ b) = showParen (d > 6) (showsPrec 7 a . showString " :+ " . showsPrec 7 b)

realPart :: Complex a -> a
realPart (a :+ _) = a

imagPart :: Complex a -> a
imagPart (_ :+ b) = b

conjugate :: Num a => Complex a -> Complex a
conjugate (a :+ b) = a :+ negate b

magnitude :: Floating a => Complex a -> a
magnitude (a :+ b) = sqrt (a * a + b * b)

phase :: RealFloat a => Complex a -> a
phase (a :+ b) = atan2 b a



mkPolar :: Floating a => a -> a -> Complex a
mkPolar r t = (r * cos t) :+ (r * sin t)

cis :: Floating a => a -> Complex a
cis t = cos t :+ sin t

instance Num a => Num (Complex a) where
  (a :+ b) + (c :+ d) = (a + c) :+ (b + d)
  (a :+ b) - (c :+ d) = (a - c) :+ (b - d)
  (a :+ b) * (c :+ d) = (a * c - b * d) :+ (a * d + b * c)
  negate (a :+ b) = negate a :+ negate b
  abs z = z
  signum z = z
  fromInteger n = fromInteger n :+ 0
